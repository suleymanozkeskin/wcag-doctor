---
name: wcag-doctor
description: Checks WCAG 2.1 color contrast compliance (AA and AAA) for frontend projects. Audits design system CSS custom properties and scans components for contrast violations. Use when user asks to "check contrast", "audit accessibility", "check WCAG compliance", "find color contrast issues", or reviews CSS/Tailwind color tokens.
license: MIT
metadata:
  author: suleymanozkeskin
  version: 0.1.0
  filePattern:
    - "**/globals.css"
    - "**/tailwind.config.*"
    - "**/*.tsx"
    - "**/*.jsx"
  bashPattern:
    - "wcag-doctor"
    - "contrast"
    - "accessibility"
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

### Step 1: Locate the project

Identify the frontend project root. The tool auto-detects:
- CSS file with custom properties (globals.css, index.css)
- Tailwind config (tailwind.config.ts/js)
- Source directories (src/, app/, pages/, components/)

If auto-detection fails, paths can be specified with `--css` and `--tailwind-config`.

### Step 2: Run the design system audit

Start with `--system` mode. This checks all semantic color pairs defined in CSS custom properties (e.g. `--primary` / `--primary-foreground`) for both light and dark themes.

```bash
wcag-doctor --system --css path/to/globals.css
```

This is the highest-value check. It catches design system-level issues that affect every component using those tokens.

Expected output: A table showing each foreground/background pair, its contrast ratio, and whether it passes AAA, AA, or fails. Exit code 1 if any pair fails the minimum level.

### Step 3: Run component scanning

For component-level analysis, scan individual files or entire directories:

```bash
# Single file
wcag-doctor --file path/to/component.tsx --css path/to/globals.css

# Full directory scan (includes cross-file inheritance analysis)
wcag-doctor --dir path/to/src/components --css path/to/globals.css --tailwind-config path/to/tailwind.config.ts
```

The component scanner:
- Parses TSX/JSX with a full AST parser
- Extracts Tailwind classes from className, cn(), clsx(), template literals
- Extracts inline style colors (backgroundColor, color, fill)
- Detects foreground/background pairs on the same element
- Builds a cross-file component graph to detect inherited background colors
- Resolves `tsconfig.json` / `jsconfig.json` path aliases such as `@/` and `~/`
- Scopes propagated text-color extraction to the matched component body instead of scanning the whole file

### Step 4: Interpret results

For each failing pair, explain:
1. Which foreground and background colors are involved
2. The actual contrast ratio vs. the required threshold
3. Whether the issue is in the design system tokens or component-level

Suggest fixes using the project's existing design system tokens when possible. If raw Tailwind colors (e.g. `bg-red-50`, `text-amber-900`) are used, recommend replacing with semantic tokens.

### Step 5: JSON output for CI

For CI integration, use `--json` to get structured output:

```bash
wcag-doctor --system --css path/to/globals.css --json
```

The JSON includes a summary with pass/fail counts and detailed results per pair.

## CLI Reference

```
wcag-doctor [OPTIONS]

OPTIONS:
  --system                    Audit design system (CSS custom property pairs)
  --file <PATH>               Scan a specific component file
  --dir <PATH>                Scan all components in directory recursively
  --css <PATH>                Path to CSS file with custom properties
  --tailwind-config <PATH>    Path to Tailwind config file
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
- Variant prefix stripping (dark:, hover:, sm:, etc.)
- Tailwind v4 detection with a warning when the built-in v3 fallback palette may be incomplete

### WCAG 2.1 thresholds
- AAA: 7:1 for normal text, 4.5:1 for large text
- AA: 4.5:1 for normal text, 3:1 for large text
- AA Large: 3:1 for large text only

## Known Limits

- Dynamic/computed runtime classes cannot always be resolved statically
- Tailwind v4 projects are detected, but the built-in fallback palette is still v3-oriented
- Static analysis can miss runtime-only theme or state combinations that are not present in source

## Common Issues

### No CSS file found
If auto-detection fails, specify the path explicitly:
```bash
wcag-doctor --system --css src/styles/globals.css
```

### No pairs detected in component scan
This means no element had both a foreground and background color class on the same element. Try `--dir` mode which also builds a cross-file component graph to detect inherited color pairs.

### Border contrast failures
Border colors (--border on --background) commonly fail because borders are UI chrome, not text. These are expected — borders only need 3:1 contrast per WCAG "non-text contrast" (SC 1.4.11), not the 4.5:1 text threshold. Use `--level aa` and assess border failures case by case.
