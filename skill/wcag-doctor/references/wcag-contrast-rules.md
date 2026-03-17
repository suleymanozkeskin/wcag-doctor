# WCAG 2.1 Contrast Rules Reference

## Relative Luminance Formula

For each sRGB channel (0-255), linearize:
- If c <= 0.04045: c_lin = c / 12.92
- Else: c_lin = ((c + 0.055) / 1.055) ^ 2.4

Relative luminance: L = 0.2126 * R_lin + 0.7152 * G_lin + 0.0722 * B_lin

## Contrast Ratio Formula

CR = (L_lighter + 0.05) / (L_darker + 0.05)

Range: 1:1 (identical) to 21:1 (black on white).

## Thresholds (WCAG 2.1 SC 1.4.3 and SC 1.4.6)

| Level | Normal text | Large text |
|-------|-------------|------------|
| AA    | 4.5:1       | 3:1        |
| AAA   | 7:1         | 4.5:1      |

Large text: >= 18pt (24px) or >= 14pt (18.67px) bold.

## Non-text contrast (SC 1.4.11)

UI components and graphical objects require 3:1 contrast against adjacent colors. This includes:
- Borders and outlines
- Focus indicators
- Icons (when they convey meaning)
- Chart elements

## Common semantic pairs to check

### Design system level
- --foreground on --background (main body text)
- --X-foreground on --X (for each semantic token: primary, secondary, card, popover, muted, accent, destructive, sidebar)
- --muted-foreground on --background (secondary text on page)
- --muted-foreground on --muted (secondary text on muted surfaces)

### Status colors on background
- --success on --background
- --error on --background
- --warning on --background
- --info on --background

### Non-text elements (3:1 minimum)
- --border on --background
- --ring on --background (focus indicators)
- --input on --background (form field borders)
