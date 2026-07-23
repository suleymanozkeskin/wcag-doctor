---
name: wcag-doctor
description: WCAG 2.1 color contrast compliance checker for frontend projects. Audits design system CSS custom properties and scans React/Next.js components for contrast violations against AA and AAA thresholds, including translucent "glass" surfaces composited over a configured backdrop and hover/focus interaction states. Use this skill whenever the user mentions contrast, accessibility, WCAG, a11y, color compliance, low-contrast placements, or asks to audit colors — even if they don't explicitly say "contrast check". Also use when reviewing globals.css, Tailwind color tokens, shadcn themes, frosted-glass/translucent surfaces, or any CSS custom property color system, since these are prime candidates for contrast issues.
---

# WCAG Doctor

WCAG 2.1 color contrast compliance checker for frontend projects. Analyzes design system tokens and component-level color pairs against AA Large (3:1), AA (4.5:1), and AAA (7:1) thresholds.

## Prerequisites

The `wcag-doctor` CLI must be installed. Check with:

```bash
wcag-doctor --version
```

If not installed:

```bash
cargo install --git https://github.com/suleymanozkeskin/wcag-doctor.git
```

## Instructions

Always use `--json` for output — it gives the full structured data without terminal truncation. The table output is for human use only and will be cut off on large projects.

### Step 1: Locate the project

Identify the frontend project root. The tool auto-detects:
- CSS file with custom properties (globals.css, index.css)
- Tailwind config (tailwind.config.ts/js)
- Source directories (src/, app/, pages/, components/)

If auto-detection fails, paths can be specified with `--css` and `--tailwind-config`.

### Step 2: Run the design system audit

Start with `--system` mode. This checks all semantic color pairs defined in CSS custom properties (e.g. `--primary` / `--primary-foreground`) for both light and dark themes.

```bash
wcag-doctor --system --css path/to/globals.css --json
```

This is the highest-value check. It catches design system-level issues that affect every component using those tokens.

### Step 2b: Translucent surfaces over a backdrop (glass / frosted UI)

Translucent surfaces (e.g. `oklch(1 0 0 / 0.62)` frosted panels) do not sit on a
token — they sit on whatever renders behind them (a wallpaper, gradient, or mesh),
so their real contrast cannot be read from the token alone. If the project has a
`wcag-doctor.json5` config it is auto-detected; otherwise pass `--config path`.

```json5
{
  backdrops: {
    light: ["var(--mesh-1)", "var(--mesh-2)", "#204050"],
    dark:  ["var(--mesh-1)", "var(--mesh-2)"],
  },
  surfaces: [
    { background: "--glass-surface", foregrounds: ["--foreground", "--muted-foreground"] },
    { background: "--glass-field",   foregrounds: ["--foreground"] },
  ],
}
```

- `backdrops` lists the backdrop's sample colors per theme (literal colors or
  `var(--token)` references). A pair passes only if it clears the **worst** sample
  — a moving backdrop must read at every point.
- `surfaces` names translucent backgrounds the `--x`/`--x-foreground` convention
  does not pair, and the foreground tokens placed on them.
- In `--json`, these pairs (and any translucent conventional pair such as
  `--sidebar`) carry `"over_backdrop": true`. Without a config, a translucent
  surface with an unknown backdrop is evaluated over black and white, worst-case.

When no config exists but the CSS clearly uses frosted/glass surfaces
(translucent `--glass-*`, `--surface-*`, or a `.liquid-backdrop`/wallpaper layer),
offer to write a `wcag-doctor.json5` so those surfaces are audited accurately.

### Step 3: Run component scanning

For component-level analysis, scan entire directories:

```bash
wcag-doctor --dir path/to/src/components --css path/to/globals.css --tailwind-config path/to/tailwind.config.ts --json
```

The component scanner:
- Parses TSX/JSX with a full AST parser
- Extracts Tailwind classes from className, cn(), clsx(), template literals
- Extracts inline style colors (backgroundColor, color, fill)
- Detects foreground/background pairs on the same element
- Extracts `hover:` / `focus:` / `focus-visible:` states as separate pairs (reported via the `state` field): a state's utilities override the base, and unchanged properties carry over, matching how `:hover`/`:focus` cascade
- Builds a cross-file component graph to detect inherited background colors
- Resolves `tsconfig.json` / `jsconfig.json` path aliases such as `@/` and `~/`

### Step 4: Parse JSON results

The JSON output has this structure:

```json
{
  "version": "0.1.0",
  "minimum_level": "AA",
  "summary": {
    "total": 52,
    "pass_aaa": 40,
    "pass_aa_large": 2,
    "pass_aa": 8,
    "fail": 2
  },
  "design_system": [
    {
      "theme": "light",
      "foreground": { "name": "--foreground", "hex": "#0f1419" },
      "background": { "name": "--background", "hex": "#ffffff" },
      "ratio": 18.51,
      "level": "AAA",
      "passes": true,
      "over_backdrop": false
    }
  ],
  "components": [
    {
      "file": "src/components/card.tsx",
      "line": 42,
      "element": "p",
      "theme": "dark",
      "state": "hover",
      "foreground": { "name": "text-gray-400", "hex": "#9ca3af" },
      "background": { "name": "bg-white", "hex": "#ffffff" },
      "ratio": 2.54,
      "level": "Fail",
      "passes": false
    }
  ],
  "warnings": [
    "CSS variable --accent has no dark theme override. Light mode value will be used in dark mode, which may cause contrast issues."
  ]
}
```

The `warnings` array is omitted when empty. It reports CSS variables with no `.dark` override.

To extract failures: filter items where `"passes": false`. Use `"theme"` to distinguish light vs. dark findings, `"state"` (`base` | `hover` | `focus` | `focus-visible`) to distinguish a resting-state failure from an interaction-only one, and `"over_backdrop": true` to mark design-system pairs whose translucent background was composited over the configured backdrop.

### Step 5: Interpret results

For each failing pair, explain:
1. Which foreground and background colors are involved
2. The actual contrast ratio vs. the required threshold
3. Whether the issue is in the design system tokens or component-level

Suggest fixes using the project's existing design system tokens when possible. If raw Tailwind colors (e.g. `bg-red-50`, `text-amber-900`) are used, recommend replacing with semantic tokens.

**Note on border failures:** Border colors (`--border` on `--background`) commonly fail the 4.5:1 text threshold. This is expected — borders are non-text UI chrome and only need 3:1 per WCAG SC 1.4.11. Flag these separately and don't count them as real failures.

**Note on exit codes:** The tool exits with code 1 if any pair fails the minimum level. Border-only failures will still trigger exit code 1. Use `--json` and filter by `passes: false` to distinguish real failures from expected border noise.

## CLI Reference

```
wcag-doctor [OPTIONS]

OPTIONS:
  --system                    Audit design system (CSS custom property pairs)
  --file <PATH>               Scan a specific component file
  --dir <PATH>                Scan all components in directory recursively
  --css <PATH>                Path to CSS file with custom properties
  --tailwind-config <PATH>    Path to Tailwind config file
  --config <PATH>             Path to wcag-doctor.json5 (backdrops + surfaces; auto-detected)
  --theme light|dark|both     Which theme to check (default: both)
  --level aa-large|aa|aaa     Minimum passing level (default: aa)
  --json                      Output as JSON
  --verbose                   Show diagnostic info
```

## What it checks

### Color formats supported
- Hex (#fff, #rrggbb, #rrggbbaa)
- RGB/RGBA (rgb(), rgba(), modern space syntax)
- HSL/HSLA (hsl(), hsla(), modern space syntax)
- Bare HSL (204 88% 24.1% - the shadcn/Tailwind convention)
- oklch()
- CSS named colors (148 colors)
- CSS variable references (var(--name), hsl(var(--name)))

### Tailwind resolution
- Semantic tokens from tailwind.config.ts/js
- Default Tailwind v3 palette (all 22 color families)
- Opacity modifiers (e.g. primary/50)
- Theme-aware variant handling: `dark:` classes are excluded in light mode, `light:` in dark mode. In dark mode, `dark:bg-X` overrides unprefixed `bg-Y` (matching Tailwind CSS specificity).
- Interaction-state variants (`hover:`, `focus:`, `focus-visible:`) are bucketed per state and reported via the `state` field, rather than stripped. Other non-color variants (`sm:`, `group-*:`, etc.) are stripped normally.
- Translucent backgrounds and a configured backdrop: see "Backdrop & surface config" — `--system` composites translucent surfaces over the backdrop; oklch/rgb/hsl alpha is preserved through resolution.
- Tailwind v4 detection with a warning when the built-in v3 fallback palette may be incomplete

### WCAG 2.1 thresholds
- AAA: 7:1 for normal text, 4.5:1 for large text
- AA: 4.5:1 for normal text, 3:1 for large text
- AA Large: 3:1 for large text only

## Known Limits

- Dynamic/computed runtime classes cannot always be resolved statically
- Tailwind v4 projects are detected, but the built-in fallback palette is still v3-oriented
- Static analysis can miss runtime-only theme or state combinations that are not present in source
- Class strings inside `cva()`/`tv()` variant maps are not yet scanned, so states defined only in a variant definition (e.g. a shared Button's `hover:` classes) are not caught at the component level
- The component scanner evaluates a translucent background over black/white worst-case; only the `--system` audit composites translucent surfaces over the configured backdrop
- Non-text contrast (WCAG SC 1.4.11, 3:1 for UI components, borders, and adjacent surfaces) is not modeled separately — border/ring pairs are reported at the text threshold (see the border-failures note above)
