export function Markdown({ source }: { source: string }) {
  const html = toHtml(source);
  return <div dangerouslySetInnerHTML={{ __html: html }} />;
}

function escape(s: string) {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function toHtml(md: string) {
  const lines = md.split("\n");
  const out: string[] = [];
  let inCode = false;
  let inTable = false;
  for (const line of lines) {
    if (line.startsWith("```")) {
      if (inCode) {
        out.push("</code></pre>");
        inCode = false;
      } else {
        out.push("<pre><code>");
        inCode = true;
      }
      continue;
    }
    if (inCode) {
      out.push(escape(line));
      continue;
    }
    if (line.startsWith("|") && line.includes("|")) {
      if (!inTable) {
        out.push("<table>");
        inTable = true;
      }
      if (/^\|[\s:-|]+\|$/.test(line.replace(/\s/g, " "))) continue;
      const cells = line.split("|").slice(1, -1);
      const tag = out[out.length - 1] === "<table>" ? "th" : "td";
      out.push(
        "<tr>" +
          cells.map((c) => `<${tag}>${inline(c.trim())}</${tag}>`).join("") +
          "</tr>",
      );
      continue;
    }
    if (inTable) {
      out.push("</table>");
      inTable = false;
    }
    if (line.startsWith("# ")) out.push(`<h1>${inline(line.slice(2))}</h1>`);
    else if (line.startsWith("## ")) out.push(`<h2>${inline(line.slice(3))}</h2>`);
    else if (line.startsWith("### ")) out.push(`<h3>${inline(line.slice(4))}</h3>`);
    else if (line.startsWith("- ")) out.push(`<li>${inline(line.slice(2))}</li>`);
    else if (line.trim() === "") out.push("");
    else out.push(`<p>${inline(line)}</p>`);
  }
  if (inCode) out.push("</code></pre>");
  if (inTable) out.push("</table>");
  return out.join("\n");
}

function inline(s: string) {
  return escape(s)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');
}
