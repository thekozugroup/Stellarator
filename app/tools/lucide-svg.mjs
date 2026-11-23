// Writes lucide icons as SVG files: 16 px, 1.75 px stroke (DESIGN.md, Iconography), drawn in black
// so the app tints them.
import { writeFileSync } from "node:fs";
const [dir, out, ...names] = process.argv.slice(2);
for (const name of names) {
  const { __iconData } = await import(`${dir}/${name}.mjs`);
  const body = __iconData.node
    .map(([tag, attrs]) => {
      const a = Object.entries(attrs).filter(([k]) => k !== "key").map(([k, v]) => `${k}="${v}"`).join(" ");
      return `  <${tag} ${a}/>`;
    })
    .join("\n");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">\n${body}\n</svg>\n`;
  writeFileSync(`${out}/${name}.svg`, svg);
}
