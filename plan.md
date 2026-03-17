# wcag-doctor — WCAG Contrast Compliance Checker

A Rust-based CLI tool that statically analyzes frontend projects for WCAG 2.1 color contrast compliance (AA and AAA).
Published as both a standalone binary and an npm package (`npx wcag-doctor`), plus a Claude Code skill.

## Architecture

```
wcag-doctor/
├── Cargo.toml
├── src/
│   ├── main.rs                 # CLI entry point (clap)
│   ├── lib.rs                  # Library root
│   ├── color/
│   │   ├── mod.rs
│   │   ├── parser.rs           # Universal color string -> RGBA
│   │   ├── named_colors.rs     # 148 CSS named colors
│   │   ├── hsl.rs              # HSL/HSLA -> sRGB
│   │   └── oklch.rs            # oklch -> sRGB
│   ├── contrast/
│   │   ├── mod.rs
│   │   ├── wcag.rs             # Relative luminance + contrast ratio
│   │   └── levels.rs           # AA/AAA threshold classification
│   ├── resolver/
│   │   ├── mod.rs
│   │   ├── css_vars.rs         # Parse CSS custom properties (:root, .dark)
│   │   ├── tailwind.rs         # Tailwind class -> color resolution
│   │   ├── tailwind_palette.rs # Default Tailwind v3/v4 color palette
│   │   └── inline_styles.rs    # style={{ }} extraction
│   ├── scanner/
│   │   ├── mod.rs
│   │   ├── component.rs        # Single-file JSX/TSX AST scanning (SWC)
│   │   ├── graph.rs            # Cross-file component import graph
│   │   └── propagation.rs      # Color inheritance traversal down the graph
│   ├── audit/
│   │   ├── mod.rs
│   │   └── design_system.rs    # Check all semantic pairs from globals.css
│   ├── report/
│   │   ├── mod.rs
│   │   ├── terminal.rs         # Colored table output
│   │   └── json.rs             # Structured JSON output
│   └── config.rs               # Auto-detect project setup (Tailwind config location, CSS files)
│
├── skill/                      # Claude Code skill definition
│   └── wcag-doctor/
│       ├── SKILL.md
│       └── references/
│           └── wcag-contrast-rules.md
│
└── plan.md                     # This file
```

## Color Parser — Multi-Format Support

The parser normalizes **any** color string to `RGBA { r: u8, g: u8, b: u8, a: f64 }`:

| Format | Example | Strategy |
|--------|---------|----------|
| Hex (3/6/8 digit) | `#fff`, `#1a2b3c`, `#1a2b3cff` | Direct byte parsing |
| RGB/RGBA | `rgb(255, 0, 0)`, `rgb(255 0 0 / 0.5)` | Regex + parse |
| HSL/HSLA | `hsl(204, 88%, 24%)`, `hsl(204 88% 24.1%)` | HSL->RGB conversion |
| Bare HSL | `204 88% 24.1%` | Detect H S% L% pattern, same HSL->RGB |
| Named colors | `white`, `red`, `transparent` | 148-entry lookup table |
| oklch | `oklch(0.5 0.2 240)` | oklch->Lab->XYZ->sRGB conversion |
| CSS var ref | `var(--primary)` | Delegate to CSS var resolver |
| Tailwind wrap | `hsl(var(--primary))` | Unwrap, resolve var, then parse HSL |

Unparseable values return `None` with a diagnostic — never panic.

## Contrast Calculation — WCAG 2.1

1. **Linearize sRGB**: For each channel c in [0,1]: if c <= 0.04045 then c/12.92, else ((c+0.055)/1.055)^2.4
2. **Relative luminance**: L = 0.2126*R + 0.7152*G + 0.0722*B
3. **Contrast ratio**: (L_lighter + 0.05) / (L_darker + 0.05)

Thresholds:
- **AAA**: >= 7:1 normal text, >= 4.5:1 large text (>=18pt or >=14pt bold)
- **AA**: >= 4.5:1 normal text, >= 3:1 large text

## CSS Variable Extractor

Parses CSS files to build theme-specific color maps:
- Scans for `:root { }` and `.dark { }` blocks
- Extracts `--name: value` declarations
- Handles both bare HSL (`204 88% 24.1%`) and full `hsl()` wrapped values
- Resolves `var(--other-var)` references within the same scope
- Outputs: `HashMap<String, RGBA>` per theme

## Tailwind Resolver

Two-layer resolution:

1. **Config-based**: Parse `tailwind.config.ts`/`.js` to find the theme.extend.colors mapping.
   Extract patterns like `primary: { DEFAULT: 'hsl(var(--primary))' }` and resolve through CSS vars.

2. **Default palette**: For raw Tailwind classes not in config (e.g. `bg-red-500`),
   fall back to the built-in Tailwind v3 color palette (all 22 color families, 11 shades each).

Resolves prefix classes: `bg-*`, `text-*`, `border-*`, `fill-*`, `stroke-*`, `ring-*`, `outline-*`.

## Component Scanner (SWC AST)

Uses `swc_ecma_parser` to parse TSX/JSX into AST, then walks:

1. **JSX elements** — extract `className` and `style` attributes
2. **className values**:
   - String literals: `"bg-primary text-foreground"`
   - Template literals: `` `bg-${color}` `` (flag as dynamic/unresolvable)
   - Function calls: `cn("bg-primary", conditional && "text-red-500")` — extract string args
3. **Style objects**: `{ backgroundColor: '#fff', color: 'hsl(0, 0%, 20%)' }`
4. **Pair detection**:
   - Same element: bg-X + text-Y on one JSX element
   - Parent-child within same file: walk JSX tree downward

## Component Graph (Cross-File Inheritance)

1. Scan all `.tsx`/`.jsx` files in the project
2. For each file, extract: component exports + component imports + JSX usage
3. Build directed graph: ParentComponent -> ChildComponent (with the wrapper's bg color annotated)
4. Propagate: walk graph, carry inherited background colors to children
5. At each child, check its text colors against the inherited background

This catches: `<div className="bg-card"><Sidebar /></div>` where Sidebar's text-foreground needs to contrast with card's background.

## Design System Audit Mode (--system)

The fastest, highest-value mode. No component scanning needed:

1. Parse globals.css for all CSS custom properties
2. Auto-detect semantic pairs:
   - Explicit: `--primary` / `--primary-foreground`
   - Implicit: `--foreground` on `--background`, `--card-foreground` on `--card`
   - Table-specific, sidebar-specific, etc.
3. Check contrast for both `:root` (light) and `.dark` themes
4. Report all pairs with their ratios and AA/AAA status

## Reporter

### Terminal (default)
Colored table with:
- Theme column (Light / Dark)
- Foreground + Background color names and resolved hex values
- Contrast ratio (e.g. 4.52:1)
- Status: PASS AAA (green), PASS AA (yellow), FAIL (red)
- Summary line: X/Y pairs pass AA, Z/Y pass AAA

### JSON (--json)
```json
{
  "version": "0.1.0",
  "minimum_level": "AA",
  "summary": { "total": 42, "pass_aaa": 30, "pass_aa": 38, "fail": 4 },
  "design_system": [
    {
      "theme": "light",
      "foreground": { "name": "--primary-foreground", "hex": "#ffffff" },
      "background": { "name": "--primary", "hex": "#0c4a6e" },
      "ratio": 8.59,
      "level": "AAA",
      "passes": true
    }
  ],
  "components": [
    {
      "file": "src/components/card.tsx",
      "line": 12,
      "element": "div",
      "foreground": { "name": "text-foreground", "hex": "#0a0a0a" },
      "background": { "name": "bg-card", "hex": "#ffffff" },
      "ratio": 19.86,
      "level": "AAA",
      "passes": true
    }
  ]
}
```

## CLI Interface

```
wcag-doctor [OPTIONS]

OPTIONS:
  --system                  Audit design system (globals.css semantic pairs)
  --file <PATH>             Scan a specific component file
  --dir <PATH>              Scan all components in directory (recursive)
  --css <PATH>              Path to CSS file with custom properties (auto-detected if omitted)
  --tailwind-config <PATH>  Path to Tailwind config (auto-detected if omitted)
  --theme <light|dark|both> Which theme to check [default: both]
  --level <aa|aaa>          Minimum passing level [default: aa]
  --json                    Output as JSON
  --verbose                 Show resolved color values and diagnostic info
  --help                    Print help
  --version                 Print version
```

## Distribution

### Crate (crates.io)
- `cargo install wcag-doctor`

### npm (npmjs.com)
- `npx wcag-doctor` — thin wrapper that downloads the platform binary on first run
- Supports: macOS arm64/x64, Linux x64/arm64, Windows x64

### Claude Code Skill
- `~/.claude/skills/wcag-doctor/SKILL.md`
- Instructs Claude to run the binary, interpret results, suggest fixes using project's design tokens

## Dependencies

- `clap` — CLI argument parsing
- `swc_ecma_parser` + `swc_ecma_ast` + `swc_common` — TSX/JSX AST parsing
- `serde` + `serde_json` — JSON serialization
- `regex` — color format detection
- `colored` — terminal colors
- `globwalk` — recursive file discovery
- `rayon` — parallel file processing (large codebases)
