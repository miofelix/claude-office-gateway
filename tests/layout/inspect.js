export function inspectLayout() {
  const failures = [];
  const check = (condition, message) => {
    if (!condition) failures.push(message);
  };
  const main = document.querySelector(".page-scroll");
  const header = document.querySelector(".window-header");
  const dialog = document.querySelector("dialog[open]");
  const root = document.documentElement;
  const shell = document.querySelector(".app-shell");
  const rect = (element) => element.getBoundingClientRect();
  const visible = (element) => element.getClientRects().length > 0;
  const overlaps = (a, b) =>
    a.left < b.right - 1 &&
    b.left < a.right - 1 &&
    a.top < b.bottom - 1 &&
    b.top < a.bottom - 1;
  check(
    root.scrollHeight <= innerHeight + 1,
    `Document creates an outer scrollbar: ${root.scrollHeight} > ${innerHeight}`,
  );
  check(
    root.scrollWidth <= innerWidth + 1,
    `Document overflows horizontally: ${root.scrollWidth} > ${innerWidth}`,
  );
  check(
    shell.scrollHeight <= shell.clientHeight + 1,
    "App shell has content outside the main scroller",
  );
  check(
    main.scrollWidth <= main.clientWidth + 1,
    "Main content overflows horizontally",
  );
  check(
    Math.abs(rect(main).bottom - innerHeight) <= 1,
    "Main scroller does not end at the viewport edge",
  );
  check(
    !overlaps(
      rect(document.querySelector(".brand")),
      rect(document.querySelector(".header-actions")),
    ),
    "Header controls overlap the brand",
  );
  for (const element of document.querySelectorAll(
    ".window-header, .section, .notice, dialog[open], .update-dialog-actions, .dialog-actions",
  )) {
    if (visible(element))
      check(
        element.scrollWidth <= element.clientWidth + 1,
        `${element.className || element.tagName} overflows horizontally: ${element.scrollWidth} > ${element.clientWidth}`,
      );
  }
  const details = document.querySelector(".trust-details");
  check(
    !overlaps(
      rect(details),
      rect(document.querySelector(".certificate-actions")),
    ),
    "Certificate trust details overlap the actions",
  );
  if (dialog) {
    const box = rect(dialog);
    check(
      box.top >= -1 &&
        box.left >= -1 &&
        box.bottom <= innerHeight + 1 &&
        box.right <= innerWidth + 1,
      "Dialog extends outside the viewport",
    );
    const scrollers = [dialog, ...dialog.querySelectorAll("*")].filter(
      (element) =>
        visible(element) &&
        /auto|scroll/.test(getComputedStyle(element).overflowY) &&
        element.scrollHeight > element.clientHeight + 1,
    );
    check(scrollers.length <= 1, "Dialog has nested vertical scrollbars");
    const original = dialog.scrollTop;
    dialog.scrollTop = dialog.scrollHeight;
    const actions = dialog.querySelector(
      ".update-dialog-actions, .dialog-actions",
    );
    if (actions)
      check(
        rect(actions).bottom <= rect(dialog).bottom + 1,
        "Dialog actions cannot be reached through its scroller",
      );
    dialog.scrollTop = original;
  } else {
    const original = main.scrollTop;
    main.scrollTop = main.scrollHeight;
    check(
      rect(document.querySelector(".page-footer")).bottom <=
        rect(main).bottom + 1,
      "Page footer cannot be reached through the main scroller",
    );
    check(rect(header).top >= -1, "Header scrolls out of the window");
    main.scrollTop = original;
  }
  return { width: innerWidth, height: innerHeight, failures };
}
