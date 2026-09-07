import { useRef, useState } from "react";
import { CURL, REPO, sitePath } from "../site";
import { RELEASE_TAG, RELEASE_VERSION } from "../release";

const release = `${REPO}/releases/download/${RELEASE_TAG}/omadesign-${RELEASE_VERSION}`;

export function Install() {
  const [status, setStatus] = useState<"idle" | "copied" | "selected">("idle");
  const command = useRef<HTMLElement>(null);
  async function copy() {
    try {
      await navigator.clipboard.writeText(CURL);
      setStatus("copied");
    } catch {
      if (command.current) {
        const selection = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(command.current);
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
      setStatus("selected");
    }
  }
  return (
    <div className="hero-install" id="install" data-motion>
      <p className="install-speed">get started in under 1m</p>
      <div className="install-command">
        <span aria-hidden="true">$</span>
        <code ref={command}>{CURL}</code>
        <button type="button" onClick={copy}>
          {status === "copied" ? "Copied ✓" : "Copy"}
        </button>
      </div>
      <div className="download-buttons">
        <a
          className="button"
          href={`${release}-x86_64-unknown-linux-gnu.tar.gz`}
          title="Omarchy on x86_64 Linux"
        >
          Omarchy <span aria-hidden="true">↓</span>
        </a>
        <a
          className="button button-outline"
          href={`${release}-aarch64-unknown-linux-gnu.tar.gz`}
          title="Omarchy on Apple Silicon running Asahi Linux"
        >
          Omarchy MX Mac <span aria-hidden="true">↓</span>
        </a>
        <a
          className="button button-outline"
          href={`${REPO}/archive/refs/tags/${RELEASE_TAG}.zip`}
        >
          Download src <span aria-hidden="true">↗</span>
        </a>
      </div>
      <p className="install-version">
        Downloads: {RELEASE_TAG}. For layered files and RAW, {" "}
        <a href={`${sitePath("docs/")}#development-preview`}>try the development preview</a>.
      </p>
      <p className="copy-status" role="status">
        {status === "selected"
          ? "Command selected. Use your browser’s Copy command."
          : status === "copied"
            ? "Installer command copied."
            : ""}
      </p>
    </div>
  );
}
