import { readFileSync } from "node:fs";
import path from "node:path";
import { inflateSync } from "node:zlib";
import { describe, expect, it } from "vitest";

const webRoot = path.resolve(__dirname, "../..");

function source(relativePath: string): string {
  return readFileSync(path.join(webRoot, relativePath), "utf8");
}

// The supplied logo is an 8-bit indexed PNG. At x=0, y=0, PNG row filters
// have no previous pixel or row, so the first decompressed byte is its palette index.
function logoCornerColor(): string {
  const png = readFileSync(path.join(webRoot, "public/logo.png"));
  expect(png.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  expect(png[24]).toBe(8);
  expect(png[25]).toBe(3);

  let palette: Buffer | undefined;
  const imageData: Buffer[] = [];
  let offset = 8;
  while (offset < png.length) {
    const length = png.readUInt32BE(offset);
    const type = png.toString("ascii", offset + 4, offset + 8);
    const chunk = png.subarray(offset + 8, offset + 8 + length);
    if (type === "PLTE") palette = chunk;
    if (type === "IDAT") imageData.push(chunk);
    if (type === "IEND") break;
    offset += length + 12;
  }

  if (!palette || imageData.length === 0) throw new Error("Logo PNG palette or pixels missing");
  const pixels = inflateSync(Buffer.concat(imageData));
  const firstPixel = pixels.at(1);
  if (firstPixel === undefined) throw new Error("Logo PNG has no first pixel");
  const paletteOffset = firstPixel * 3;
  return `#${palette.subarray(paletteOffset, paletteOffset + 3).toString("hex")}`;
}

describe("public website background", () => {
  it("locks website field to the supplied logo background across themes", () => {
    const css = source("src/app/globals.css");
    const theme = css.match(/\.marketing-theme\s*\{([^}]+)\}/)?.[1];
    const homepage = source("src/app/page.tsx");
    const marketingLayout = source("src/app/(marketing)/layout.tsx");
    const rootLayout = source("src/app/layout.tsx");
    const header = source("src/components/sections/header.tsx");

    expect(logoCornerColor()).toBe("#0e1a2a");
    expect(css).toContain(`--brand-field: ${logoCornerColor()};`);
    expect(theme).toContain("color-scheme: dark;");
    expect(theme).toContain("--background: var(--brand-field);");
    expect(css).not.toMatch(/\.dark\s+\.marketing-theme\s*\{/);
    expect(homepage).toContain('className="marketing-theme dark relative min-h-screen');
    expect(marketingLayout).toContain('className="marketing-theme dark min-h-screen');
    expect(rootLayout).toContain('themeColor: "#0E1A2A"');
    expect(header).not.toContain("ThemeToggle");
  });
});
