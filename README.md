# wcag-doctor

WCAG 2.1 color contrast compliance checker for frontend projects. Statically analyzes design system tokens and React/Next.js components against AA and AAA contrast thresholds — without running the application.

## What it does

- Audits CSS custom property pairs (`--primary` / `--primary-foreground`) for contrast compliance
- Scans TSX/JSX components for foreground/background color pairs via AST parsing
- Resolves Tailwind classes, CSS variables, inline styles, and the default Tailwind v3 palette
- Builds a cross-file component graph to detect inherited background colors
- Supports hex, rgb, hsl, oklch, named colors, and CSS variable references
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
```

## Install as a Claude skill

This tool ships as a [Claude skill](https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/skills) so Claude can run contrast audits automatically when relevant.

### Claude Code

```bash
git clone https://github.com/suleymanozkeskin/wcag-doctor.git /tmp/wcag-doctor
cp -r /tmp/wcag-doctor/skill/wcag-doctor ~/.claude/skills/wcag-doctor
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
