"use client";

/** Plain anchors work before hydration; mounted handlers explicitly move focus. */
export function SkipNavigation() {
  return (
    <nav aria-label="Skip navigation" className="skip-navigation">
      {[
        ["main-content", "Skip to main content"],
        ["workspace-editor", "Skip to editor"],
        ["workspace-console", "Skip to console"],
        ["primary-navigation", "Skip to navigation"],
      ].map(([target, label]) => (
        <a
          key={target}
          href={`#${target}`}
          onClick={(event) => {
            const preferred = document.getElementById(target);
            const element =
              target === "primary-navigation" &&
              preferred &&
              getComputedStyle(preferred).display === "none"
                ? document.getElementById("primary-navigation-mobile")
                : preferred || document.getElementById("main-content");
            if (element) {
              event.preventDefault();
              element.focus();
              element.scrollIntoView({ block: "nearest" });
            }
          }}
        >
          {label}
        </a>
      ))}
    </nav>
  );
}
