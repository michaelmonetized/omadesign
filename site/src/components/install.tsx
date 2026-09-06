import { useRef, useState } from "react";
import { CURL, REPO, sitePath } from "../site";
import { Arrow, Eyebrow } from "./studio-ui";

export function Install() {
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const command = useRef<HTMLElement>(null);
  async function copy() {
    try {
      await navigator.clipboard.writeText(CURL);
      setCopied(true);
      setCopyError(false);
    } catch {
      setCopyError(true);
      if (command.current) {
        const selection = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(command.current);
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
    }
  }
  return (
    <section className="install-section" id="install">
      <div className="shell section">
        <Eyebrow>YOUR NEXT GOOD IDEA STARTS HERE</Eyebrow>
        <h2>
          Make something
          <br />
          <span>only you could make.</span>
        </h2>
        <p>
          A native creative home on Linux.
          <br />
          Free to use. Free to inspect. Free to make your own.
        </p>
        <div className="button-row">
          <a className="button" href={`${REPO}/releases`}>
            Download for Linux <Arrow diagonal />
          </a>
          <a
            className="button button-outline"
            href={sitePath("docs/contributing")}
          >
            Build the current studio <Arrow />
          </a>
        </div>
        <div className="install-platforms">
          <span>ARM64 / aarch64</span>
          <span>x86_64</span>
          <span>MIT licensed</span>
        </div>
        <div className="install-command">
          <span aria-hidden="true">$</span>
          <code ref={command}>{CURL}</code>
          <button type="button" onClick={copy}>
            {copied ? "Copied ✓" : "Copy"}
          </button>
        </div>
        <p className="copy-status" role="status">
          {copyError
            ? "Command selected. Use your browser’s Copy command."
            : copied
              ? "Installer command copied."
              : "One command installs the latest published package in ~/.local/bin."}
        </p>
        <p className="release-note">
          <strong>An alpha with momentum.</strong> The film and feature tour
          show the September 5 source. Published v0.0.1-alpha.rc packages are
          from September 2 and predate these additions. Build the current source
          to try everything shown here, or check release notes before
          downloading.
        </p>
      </div>
    </section>
  );
}
