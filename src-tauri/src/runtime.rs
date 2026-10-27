use crate::{config::GatewayConfig, gateway::GatewayRouter};
use axum_server::{tls_rustls::RustlsConfig, Handle};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinHandle};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Stopped,
    Starting,
    Checking,
    NeedsTrust,
    Ready,
    Failed,
}
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub phase: Phase,
    pub running: bool,
    pub verified_at: Option<String>,
    pub verification_ms: Option<u64>,
    pub error: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            phase: Phase::Stopped,
            running: false,
            verified_at: None,
            verification_ms: None,
            error: None,
        }
    }
}
struct Control {
    snapshot: Snapshot,
    generation: u64,
    handle: Option<Handle>,
    task: Option<JoinHandle<()>>,
    router: Option<GatewayRouter>,
    cancel: watch::Sender<bool>,
}
#[derive(Clone)]
pub struct Runtime {
    inner: Arc<Mutex<Control>>,
}
impl Default for Runtime {
    fn default() -> Self {
        let (cancel, _) = watch::channel(false);
        Self {
            inner: Arc::new(Mutex::new(Control {
                snapshot: Snapshot::default(),
                generation: 0,
                handle: None,
                task: None,
                router: None,
                cancel,
            })),
        }
    }
}
impl Runtime {
    pub fn snapshot(&self) -> Snapshot {
        self.inner.lock().unwrap().snapshot.clone()
    }
    pub fn busy(&self) -> bool {
        let s = self.snapshot();
        s.running || matches!(s.phase, Phase::Starting | Phase::Checking)
    }
    fn update(&self, generation: u64, f: impl FnOnce(&mut Control)) {
        let mut state = self.inner.lock().unwrap();
        if state.generation == generation {
            f(&mut state);
        }
    }
    pub fn needs_trust(&self) {
        let mut state = self.inner.lock().unwrap();
        state.snapshot.phase = Phase::NeedsTrust;
        state.snapshot.error =
            Some("Install and trust the local certificate before connecting.".into());
    }
    pub async fn stop(&self) {
        let task = {
            let mut state = self.inner.lock().unwrap();
            state.generation += 1;
            let _ = state.cancel.send(true);
            if let Some(handle) = state.handle.take() {
                handle.shutdown();
            }
            state.router = None;
            state.snapshot = Snapshot::default();
            state.task.take()
        };
        if let Some(mut task) = task {
            if tokio::time::timeout(Duration::from_secs(3), &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
    }
    pub async fn connect(
        &self,
        config: GatewayConfig,
        router: GatewayRouter,
        cert: &Path,
        key: &Path,
    ) -> Result<(), String> {
        let (generation, mut cancelled) = {
            let mut state = self.inner.lock().unwrap();
            if state.snapshot.running
                || matches!(state.snapshot.phase, Phase::Starting | Phase::Checking)
            {
                return Err("A gateway operation is already active.".into());
            }
            state.generation += 1;
            state.snapshot = Snapshot {
                phase: Phase::Starting,
                ..Default::default()
            };
            let (cancel, rx) = watch::channel(false);
            state.cancel = cancel;
            (state.generation, rx)
        };
        let result=async {
            let tls=RustlsConfig::from_pem_file(cert,key).await.map_err(|_|"Could not load the local certificate identity. Reinstall it in settings.".to_string())?;
            let listener=std::net::TcpListener::bind(("127.0.0.1",config.port)).map_err(|e|if e.kind()==std::io::ErrorKind::AddrInUse {"The local port is in use. Stop the other gateway or change the port in Advanced settings.".to_string()} else {format!("Could not bind the localhost port: {e}")})?;
            listener.set_nonblocking(true).map_err(|e|e.to_string())?;
            if *cancelled.borrow() {return Ok(());}
            let handle=Handle::new();let server_handle=handle.clone();let runtime=self.clone();let app=router.clone().app();
            let task=tokio::spawn(async move {
                let result=axum_server::from_tcp_rustls(listener,tls).handle(server_handle).serve(app.into_make_service()).await;
                runtime.update(generation,|state|{if !state.snapshot.running && state.snapshot.phase==Phase::Failed {return;}state.snapshot.running=false;state.snapshot.phase=Phase::Failed;state.snapshot.verified_at=None;state.snapshot.error=Some(if result.is_err(){"The local HTTPS listener failed. Reconnect to continue."}else{"The local gateway stopped unexpectedly. Reconnect to continue."}.into());});
            });
            self.update(generation,|state|{state.handle=Some(handle.clone());state.task=Some(task);state.router=Some(router.clone());});
            let listening=tokio::select!{_ = cancelled.changed()=>return Ok(()), result=handle.listening()=>result};
            if listening.is_none(){return Err("Could not start the localhost HTTPS listener.".into());}
            self.update(generation,|state|{state.snapshot.running=true;state.snapshot.phase=Phase::Checking;});
            let started=Instant::now();
            tokio::select! {_=cancelled.changed()=>return Ok(()), result=router.verify_upstream()=>result.map_err(|e|e.to_string())?};
            self.update(generation,|state|{state.snapshot.phase=Phase::Ready;state.snapshot.running=true;state.snapshot.verified_at=Some(chrono::Utc::now().to_rfc3339());state.snapshot.verification_ms=Some(started.elapsed().as_millis() as u64);state.snapshot.error=None;});
            Ok(())
        }.await;
        if let Err(error) = result {
            self.update(generation, |state| {
                if let Some(handle) = state.handle.take() {
                    handle.shutdown();
                }
                state.snapshot.running = false;
                state.snapshot.phase = Phase::Failed;
                state.snapshot.error = Some(error);
                state.snapshot.verified_at = None;
            });
        }
        Ok(())
    }
    pub async fn recheck(&self) -> Result<(), String> {
        let (generation, router, mut cancel) = {
            let mut state = self.inner.lock().unwrap();
            if !state.snapshot.running
                || matches!(state.snapshot.phase, Phase::Starting | Phase::Checking)
            {
                return Err("Start the gateway before rechecking the provider.".into());
            }
            state.snapshot.phase = Phase::Checking;
            state.snapshot.error = None;
            (
                state.generation,
                state.router.clone().ok_or("No active provider")?,
                state.cancel.subscribe(),
            )
        };
        let started = Instant::now();
        let result = tokio::select! {_ = cancel.changed()=>return Ok(()), result=router.verify_upstream()=>result};
        self.update(generation, |state| match result {
            Ok(()) => {
                state.snapshot.phase = Phase::Ready;
                state.snapshot.verified_at = Some(chrono::Utc::now().to_rfc3339());
                state.snapshot.verification_ms = Some(started.elapsed().as_millis() as u64);
            }
            Err(error) => {
                state.snapshot.phase = Phase::Failed;
                state.snapshot.error = Some(error.to_string());
                state.snapshot.verified_at = None;
            }
        });
        Ok(())
    }
    pub fn shutdown(&self) {
        let mut state = self.inner.lock().unwrap();
        state.generation += 1;
        let _ = state.cancel.send(true);
        if let Some(handle) = state.handle.take() {
            handle.shutdown();
        }
    }
}
