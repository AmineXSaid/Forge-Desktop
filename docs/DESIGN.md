# Forge Desktop — design system

> A technical manual typeset for a literary journal: Monad's warm parchment
> and mono voice, General Intelligence Company's paper, serif and frosted pill.

Forge Desktop combines two references, and only these two. There are no other
sources:

| | Monad (`DESIGN_1.md`, `Monad.mp4`) | General Intelligence Company (`DESIGN_2.md`, `general_intelligence__company.mp4`) |
| --- | --- | --- |
| Canvas | Parchment `#f6f3f1`, warm | Parchment `#fefffc`, paper `#ffffff`, linen `#f9faf7` |
| Voice | Serif announces, **mono instructs** (uppercase labels) | Serif headlines, **sans** for everything utilitarian |
| Shape | Pills (`9999px`) for buttons and tags, 40px cards | 8px buttons, 12–16px cards, **50px frosted nav pill** |
| Edges | 1px Ash `#cecac8`, no shadows | 1px Mist `#dee2de` (green-tinted), soft card shadow |
| Colour | Lake Blue `#2b59d1` for the one primary action; pastel washes as decoration | Signal Blue `#41a1cf` outlines only; Dusk `#1f1f29` the one dark fill |
| Signature | Pipeline-node pills joined by thin lines; blurred coral→sky→mint washes; black announcement bar | Floating frosted pill nav; double-framed diagram card with a "Fig." caption; quiet bottom-right banner; pixel art |

**The rule of the combination:** structure comes from GI (paper, serif,
frosted pill, framed figures). The machine voice comes from Monad (mono
uppercase labels, pill tags, pipeline lines, pastel washes). Each surface takes
one decision from one source, never a blend of two values.

## 1. Colour

All values are from the two references. Nothing else may be added.

| Token | Value | From | Use |
| --- | --- | --- | --- |
| `--fd-chrome` | `#f6f3f1` | Monad Parchment | Title bar, sidebar, Changes pane: the app's frame |
| `--fd-canvas` | `#fefffc` | GI Parchment | The chat's reading surface |
| `--fd-paper` | `#ffffff` | GI Paper | Cards: composer, tool output, permission card, menus, diff figure |
| `--fd-linen` | `#f9faf7` | GI Linen | Inputs, user message, code blocks, quiet fills |
| `--fd-ink` | `#242424` | Monad Off-Black | Primary text, icons |
| `--fd-graphite` | `#2c2c2c` | GI Graphite | Serif headings |
| `--fd-body` | `#444141` | GI Charcoal | Body prose |
| `--fd-muted` | `#797776` | Monad Smoke | Meta text, captions, placeholders |
| `--fd-faint` | `#b4b8b4` | GI Fog | Disabled, rule ticks |
| `--fd-edge-chrome` | `#cecac8` | Monad Ash | Hairlines on parchment |
| `--fd-edge-paper` | `#dee2de` | GI Mist | Hairlines on paper and canvas |
| `--fd-primary` | `#2b59d1` | Monad Lake Blue | **The one primary action** per surface: Send, Allow |
| `--fd-signal` | `#41a1cf` | GI Signal Blue | Outlines, links, focus rings, the selected edge, never a fill |
| `--fd-dusk` | `#1f1f29` | GI Dusk | The dark pill: New session, the active segment of the nav pill |
| `--fd-twilight` | `#282834` | GI Twilight | Outlined secondary buttons, icon strokes |
| `--fd-selected` | `#cfdaf5` | Monad Periwinkle Mist | Selected rows (session, file) |
| `--fd-cerulean` | `#0081c0` | GI Cerulean | Rare: the "working" pulse |
| `--fd-coral` / `--fd-sky` / `--fd-mint` / `--fd-gold` | `#ff9473` / `#a0b5eb` / `#a7fccd` / `#ecda98` | Monad pastels | Washes only: empty-state glow, working glow, diff surfaces at low alpha |
| `--fd-black` | `#000000` | Monad Ink | The announcement bar (notices that need you) |

Diff colours come from the pastels: added lines sit on mint, removed lines on
coral, both at low alpha, and the text stays ink. There is no red or green
outside this.

## 2. Type

| Role | Family | Size / line / tracking | From |
| --- | --- | --- | --- |
| Display (empty-state headline) | Serif 400 | 40px / 1.1 / -0.02em | GI heading, Monad heading |
| Heading (session title, pane titles, prompt question) | Serif 400 | 18–24px / 1.2 / -0.02em | Monad subheading |
| Body (chat prose, inputs, rows) | Sans 400 | 13–14px / 1.5 / -0.01em | GI af body |
| Label (buttons, tags, nav, section labels, tool names, badges, shortcuts) | **Mono 400/500, UPPERCASE** | 11–12px / 1.2 / -0.02em | Monad caption and label |
| Meta (timestamps, paths, captions) | Mono 400 | 11px / 1.3 | Monad caption |
| Code | Mono 400, normal case | 12px / 1.55 | Monad |

- Serif weight is always 400. Neither reference ever bolds it.
- Mono is never used for sentences longer than a label, except code.
- The faces are the references' own named substitutes, bundled, never system:
  - serif: **Fraunces** (GI's ppmondwest; also stands in for Monad's Untitled Serif);
  - sans: **Inter** (GI's af);
  - mono: **JetBrains Mono** (Monad's ABC Diatype Mono).

## 3. Shape and space

- **Pills (`9999px`)**: every button, tag, segmented control, the nav pill and
  the tool-call pills (Monad buttons and tags; GI nav).
- **Cards**: 12px (tool output, menus, permission card); 16px (the diff figure's
  inner card, toasts); 20px (the composer). GI's 12–16px; Monad's 40px is a
  marketing size and is not used in an app.
- **Spacing**: a 4px base (GI), in steps of 4 / 8 / 12 / 16 / 24 / 32.
- **Elevation**: hairlines first. Shadows only on floating things:
  - nav pill: `0 2px 6px rgba(0,0,0,.15)` (GI nav);
  - composer and cards: `0 1px 1px rgba(0,0,0,.08), 0 4px 5px rgba(0,0,0,.08)` (GI content card);
  - menus and toasts: `0 1px 8px rgba(0,0,0,.05)` (GI diagram card).
- **Glass**: `backdrop-filter: blur(14px)` on translucent paper for the nav pill
  and toasts only (GI).

## 4. Components, mapped to the app

| App surface | Treatment | From |
| --- | --- | --- |
| Title bar | Parchment strip. Centre: a frosted **nav pill** [Forge cube · CHAT · CHANGES · TERMINAL · dark **NEW SESSION ›** segment]. Left: project name in serif with a mono branch tag. Right: window buttons | GI nav pill; Monad parchment |
| Sidebar | Parchment. Mono uppercase section label `SESSIONS`. Rows in sans; selected row periwinkle; status as a small dot | Monad surfaces and labels |
| New session | Dark Dusk pill with a circled chevron `›` | GI "Get Cofounder" |
| Empty state | Serif display headline with one muted, underlined phrase; a blurred coral→sky→mint wash behind Forge's pixel hammer | GI headline treatment and pixel art; Monad wash |
| User message | Linen block, hairline Mist, 12px radius, sans | GI input/linen |
| Assistant prose | Sans on canvas, no container | GI |
| Tool call | A **pipeline-node pill**: icon + MONO UPPERCASE tool name + file path in sans, on paper with an Ash hairline, joined to the next step by a thin vertical line | Monad pipeline nodes |
| Tool output / diff in transcript | Paper card, 12px, Mist hairline | GI content card |
| Permission prompt | Paper card; the question in serif; options as rows; **Allow** is the Lake Blue pill (the one primary action), Deny an outlined Twilight pill | Monad primary pill; GI outlined |
| Composer | Paper card, 20px radius, GI content shadow; mono uppercase chips (model, mode); Send is the Lake Blue circle | GI card; Monad pill and primary |
| Changes pane | Parchment pane; file list rows with mono status letter and counts; the diff in a **double-framed figure** (translucent outer frame, white inner card) captioned `Fig. 1 · src/server.ts` | GI diagram card and caption |
| Notices that need you | Black announcement bar across the top with a white outline pill button | Monad announcement bar |
| Toasts | Bottom-right paper banner, small sans, text buttons split by `/` | GI cookie banner |
| Focus | 1px Signal Blue outline, offset 1px | GI |

## 5. Do and don't

**Do**
- Keep the screen 95% neutral: blue appears only on the one primary action
  (Lake Blue fill) and on outlines, focus and links (Signal Blue).
- Set labels in mono uppercase and sentences in sans.
- Use serif for anything that is a title.
- Use hairlines before shadows.
- Let colour appear only as washes: the pastels, behind content, blurred.

**Don't**
- Bold the serif.
- Fill anything with Signal Blue or the pastels.
- Use pure white for the app frame (it's parchment), or pure black for text.
- Use a radius above 20px on cards, or below `9999px` on buttons.
- Add a colour that isn't in §1.

## 6. Where it lives

- Tokens: `src/webview/src/styles/forge-desktop-theme.css` in FORGE, scoped to
  `body.forge-desktop`, so the VS Code extension is untouched.
- Faces: `forge-fonts.css`, files in `assets/fonts/`, with their OFL licences.
- The `--vscode-*` structure variables for the light theme are set in the same
  file, so Forge's chat components follow the palette without per-component forks.
- Component-level treatments (§4) live in the same file, under
  `body.forge-desktop`.
