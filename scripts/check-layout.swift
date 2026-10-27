import AppKit
import WebKit

@MainActor
final class LayoutRunner: NSObject, NSApplicationDelegate {
    private var window: NSWindow!
    private var webView: WKWebView!
    private var checks = 0
    private var failures: [String] = []
    private let baseURL = CommandLine.arguments[1]

    func applicationDidFinishLaunching(_ notification: Notification) {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        webView = WKWebView(frame: .zero, configuration: configuration)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1060, height: 800),
                          styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = webView
        window.orderBack(nil)
        Task {
            do {
                try await run()
                print("WKWebView layout: \(checks) checks, \(failures.count) failures")
                for failure in failures { print(failure) }
                exit(failures.isEmpty ? 0 : 1)
            } catch {
                print("WKWebView layout failed: \(error)")
                exit(1)
            }
        }
    }

    private func evaluate(_ script: String) async throws -> Any? {
        try await webView.evaluateJavaScript(script)
    }

    private func load(platform: String, language: String, theme: String, scenario: String = "stopped") async throws {
        var components = URLComponents(string: baseURL + "/tests/layout/")!
        components.queryItems = ["platform": platform, "language": language,
                                 "theme": theme, "scenario": scenario].map {
            URLQueryItem(name: $0.key, value: $0.value)
        }
        _ = try await evaluate("window.inspectLayout = undefined")
        webView.load(URLRequest(url: components.url!))
        for _ in 0..<300 {
            try await Task.sleep(for: .milliseconds(20))
            if let ready = try? await evaluate("!!window.inspectLayout && document.querySelector('.header-actions button')?.disabled === false"),
               ready as? Bool == true { return }
        }
        throw NSError(domain: "Layout fixture did not become ready", code: 1)
    }

    private func inspect(_ label: String, width: Int, height: Int = 560) async throws {
        window.setContentSize(NSSize(width: width, height: height))
        try await Task.sleep(for: .milliseconds(20))
        let result = try await evaluate("window.inspectLayout()") as! [String: Any]
        checks += 1
        if result["width"] as? Int != width || result["height"] as? Int != height {
            throw NSError(domain: "Native viewport did not resize to \(width)x\(height)", code: 1)
        }
        for failure in result["failures"] as! [String] {
            failures.append("\(label) \(width)x\(height): \(failure)")
            if failures.count == 1 {
                let diagnostic = try await evaluate("JSON.stringify([...document.querySelectorAll('.provider-section *')].filter(e => e.getClientRects().length && e.scrollWidth > e.clientWidth + 1).map(e => ({ tag: e.tagName, class: e.className, width: e.clientWidth, scrollWidth: e.scrollWidth })))")
                print("First failure details: \(diagnostic ?? "")")
            }
        }
    }

    private func run() async throws {
        for platform in ["macos", "windows"] {
            for language in ["en", "zh"] {
                for theme in ["light", "dark"] {
                    let label = "\(platform)/\(language)/\(theme)"
                    try await load(platform: platform, language: language, theme: theme)
                    for width in [360, 480, 620, 780, 920, 921, 1060, 1440] {
                        try await inspect(label, width: width)
                    }
                    try await inspect(label, width: 1060, height: 800)
                    _ = try await evaluate("document.querySelectorAll('details').forEach(element => element.open = true)")
                    for width in [360, 780, 921, 1060] {
                        try await inspect(label + "/expanded", width: width)
                    }
                }
                for scenario in ["ready", "error", "update", "update-error", "certificate", "lightweight"] {
                    try await load(platform: platform, language: language, theme: "light", scenario: scenario)
                    if scenario.hasPrefix("update") {
                        _ = try await evaluate("document.querySelector('.version-button').click()")
                    } else if scenario == "certificate" {
                        _ = try await evaluate("document.querySelector('.certificate-actions .button.secondary').click()")
                    } else if scenario == "lightweight" {
                        _ = try await evaluate("document.querySelector('.advanced-settings').open = true; const input = document.querySelector('.field input'); const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; setter.call(input, input.value + '/edited'); input.dispatchEvent(new Event('input', { bubbles: true }))")
                        try await Task.sleep(for: .milliseconds(20))
                        _ = try await evaluate("document.querySelector('.lightweight-setting button').click()")
                    }
                    try await Task.sleep(for: .milliseconds(20))
                    if ["update", "update-error", "certificate", "lightweight"].contains(scenario) {
                        let open = try await evaluate("!!document.querySelector('dialog[open]')") as? Bool
                        if open != true { throw NSError(domain: "Expected \(scenario) dialog to open", code: 1) }
                    }
                    if scenario == "ready" {
                        _ = try await evaluate("document.querySelector('.copy-button[aria-label=\"Copy Gateway token\"]').click()")
                    }
                    for width in [360, 780, 1060] {
                        try await inspect("\(platform)/\(language)/\(scenario)", width: width)
                    }
                }
            }
        }
    }
}

MainActor.assumeIsolated {
    let app = NSApplication.shared
    app.setActivationPolicy(.accessory)
    let runner = LayoutRunner()
    app.delegate = runner
    app.run()
}
