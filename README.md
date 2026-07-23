# wcag-doctor

WCAG 2.1 color contrast compliance checker for frontend projects. Statically analyzes design system tokens and React/Next.js components against AA and AAA contrast thresholds — without running the application.

## What it does

- Audits CSS custom property pairs (`--primary` / `--primary-foreground`) for contrast compliance
- Scans TSX/JSX components for foreground/background color pairs via AST parsing
- Resolves Tailwind classes, CSS variables, inline styles, and the default Tailwind v3 palette
- Builds a cross-file component graph to detect inherited background colors
- Composites translucent surfaces (frosted "glass" over a wallpaper/mesh) over a configured backdrop, reporting the worst-case contrast across the backdrop's luminance range
- Extracts `hover:` / `focus:` / `focus-visible:` states as distinct pairs, so state-only contrast regressions are caught
- Supports hex, rgb, hsl, oklch (incl. alpha), named colors, and CSS variable references
- Checks both light and dark themes

## Install the CLI

Requires [Rust](https://rustup.rs/):

```bash
cargo install --git https://github.com/suleymanozkeskin/wcag-doctor.git
```

## Usage

```bash
# Audit design system tokens
wcag-doctor --system --css src/app/globals.css

# Scan a component directory
wcag-doctor --dir src/components --css src/app/globals.css

# JSON output for CI
wcag-doctor --system --css src/app/globals.css --json

# Check only dark theme at AAA level
wcag-doctor --system --css src/app/globals.css --theme dark --level aaa

# Audit translucent surfaces over a backdrop (config auto-detected, or --config)
wcag-doctor --system --css src/app/globals.css --config wcag-doctor.json5
```

## Backdrop & surface config (optional)

Translucent surfaces do not sit on a token — they sit on whatever renders behind
them (a wallpaper, gradient, or animated mesh). Their real contrast depends on
that backdrop, so it cannot be read from the token alone. Declare it in a
`wcag-doctor.json5` at the project root (auto-detected) or pass `--config`:

```json5
{
  // Backdrop sample colors per theme: literal colors or var(--token) references.
  // A pair passes only if it clears the worst sample (a moving backdrop must
  // read at every point).
  backdrops: {
    light: ["var(--mesh-1)", "var(--mesh-2)", "#204050"],
    dark: ["var(--mesh-1)", "var(--mesh-2)"],
  },
  // Translucent surfaces the --x/--x-foreground convention does not pair, plus
  // the foreground tokens placed on them. Composited over the backdrop unless
  // `over_backdrop: false`.
  surfaces: [
    { background: "--glass-surface", foregrounds: ["--foreground", "--muted-foreground"] },
    { background: "--glass-field", foregrounds: ["--foreground"] },
  ],
}
```

Without a config file, behavior is unchanged: a translucent surface with an
unknown backdrop is evaluated over black and white, worst-case.

## Install as a Claude skill

This tool ships as a [Claude skill](https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/skills) so Claude can run contrast audits automatically when relevant.

### Claude Code

In Claude Code, run:

```
/plugin marketplace add suleymanozkeskin/wcag-doctor
/plugin install wcag-doctor@wcag-doctor
```

### Claude.ai

1. Download the `skill/wcag-doctor/` folder from this repo
2. Zip it
3. Go to **Settings > Capabilities > Skills** and upload the zip

### Project-level (for your team)

Copy the skill into your repo so all team members get it:

```bash
cp -r /tmp/wcag-doctor/skill/wcag-doctor .claude/skills/wcag-doctor
```

## License

MIT
