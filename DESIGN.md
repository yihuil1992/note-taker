---
name: Note Taker
description: Local-first meeting transcription workspace using the Hoshikuzu quiet archival star atlas style.
colors:
  deep-space: "#02040a"
  ink-night: "#05070d"
  observatory-panel: "#101827"
  starlight: "#f8fafc"
  starlight-muted: "#aeb8c7"
  starlight-faint: "#6f7b8d"
  hairline: "rgba(255,255,255,0.14)"
  hairline-strong: "rgba(170,215,220,0.34)"
  pale-cyan: "#aad7dc"
  soft-amber: "#f5f0dc"
  archive-background: "#f7f8f3"
  archive-paper: "#fffffc"
  archive-panel: "#edf0ea"
  archive-ink: "#172033"
  archive-muted: "#66727f"
  archive-accent: "#477c86"
  danger: "#f08a8a"
  warn: "#d8bd82"
typography:
  headline:
    fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "25px"
    fontWeight: 760
    lineHeight: 1.2
    letterSpacing: "0"
  title:
    fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "17px"
    fontWeight: 740
    lineHeight: 1.25
    letterSpacing: "0"
  body:
    fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "0"
  label:
    fontFamily: "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif"
    fontSize: "12px"
    fontWeight: 720
    lineHeight: 1.2
    letterSpacing: "0"
  mono:
    fontFamily: "Cascadia Mono, SFMono-Regular, Consolas, monospace"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: "0"
rounded:
  xs: "2px"
  sm: "4px"
  md: "6px"
spacing:
  xs: "6px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "18px"
components:
  button-primary:
    backgroundColor: "{colors.soft-amber}"
    textColor: "{colors.deep-space}"
    rounded: "{rounded.sm}"
    padding: "0 14px"
    height: "48px"
  button-secondary:
    backgroundColor: "{colors.ink-night}"
    textColor: "{colors.starlight}"
    rounded: "{rounded.sm}"
    padding: "0 12px"
    height: "38px"
  panel:
    backgroundColor: "{colors.ink-night}"
    textColor: "{colors.starlight}"
    rounded: "{rounded.md}"
    padding: "17px"
  input:
    backgroundColor: "{colors.ink-night}"
    textColor: "{colors.starlight}"
    rounded: "{rounded.sm}"
    padding: "0 10px"
    height: "40px"
---

# Design System: Note Taker

## Overview

**Creative North Star: "Local Observatory Console"**

Note Taker should now inherit the Hoshikuzu visual language: a quiet archival star atlas adapted into a working meeting capture console. It should feel precise, dimly lit, and intentional, as if recording, transcription, summary, and export are instruments filed into the same observatory.

The visual language is restrained and exact: deep-space surfaces, warm starlight text, pale cyan state signals, soft amber decisive actions, hairline borders, compact metadata labels, and small radii. Motion is state feedback, not performance. The user should always know what changed after a click.

**Key Characteristics:**

- Local-first trust cues stay visible without becoming banners.
- Pale cyan is reserved for selection, readiness, focus, and subtle state.
- Soft amber is reserved for the decisive recording action.
- Panels are functional annotations and instruments, never decorative card stacks.
- Desktop layout behaves like an observatory console: top atlas header, archive index, central capture instrument, session record, instrument stack, and local footer.
- Mobile layout stacks the same observatory order: header, archive, capture, detail, instruments, footer.

## Colors

The palette follows Hoshikuzu's atlas system: Night Atlas is the default blue-black workspace, while Archive Sheet is a light archival mode using paper surfaces, ink text, and a restrained teal signal.

### Primary

- **Pale Cyan Signal** (`#aad7dc`): selected state, ready dots, focus rings, active tab underline, and rare signal details. Use it under ten percent of any screen.
- **Soft Amber Action** (`#f5f0dc`): the main recording action only.
- **Archive Teal Signal** (`#477c86`): Archive Sheet's replacement for pale cyan in selected, focus, and ready states.

### Neutral

- **Deep Space** (`#02040a`): app background and starfield canvas.
- **Ink Night** (`#05070d`): main panel fill.
- **Observatory Panel** (`#101827`): source tiles, selected rows, and low-emphasis surface variation.
- **Starlight** (`#f8fafc`): primary text.
- **Muted Starlight** (`#aeb8c7`): descriptions, timestamps, metadata, and app footnotes.
- **Faint Starlight** (`#6f7b8d`): tertiary metadata.
- **Hairline** (`rgba(255,255,255,0.14)`): structural rails and panel borders.
- **Strong Hairline** (`rgba(170,215,220,0.34)`): focused inputs and selected surfaces.

### Archive Sheet

- **Archive Background** (`#f7f8f3`): light atlas canvas.
- **Archive Paper** (`#fffffc`): primary panel fill.
- **Archive Panel** (`#edf0ea`): secondary rail, tile, and hover fill.
- **Archive Ink** (`#172033`): primary text and decisive action fill.
- **Archive Muted** (`#66727f`): metadata, timestamps, descriptions, and footnotes.

### Tertiary

- **Error Red** (`#a2372d`): destructive or failed recording states only.
- **Setup Amber** (`#b87516`): setup-needed and warning indicators only.

### Named Rules

**The Stars Own The Light Rule.** Page-wide decorative glow is prohibited. Light should come from star texture, focus states, and functional annotations.

**The Ten Percent Accent Rule.** Cyan and amber are rare signals. If an accent is visible before the content is understood, it is too loud.

**The No Gradient Rule.** Gradients are prohibited in core UI. Premium comes from alignment, solid color, hairlines, readable typography, and clean state feedback.

**The Hoshikuzu Theme Rule.** Theme mode is a local UI preference expressed as `data-atlas-mode="night|archive"` on the root element. It changes semantic tokens, not layout or content.

## Typography

**Display Font:** Inter, with system UI fallbacks  
**Body Font:** Inter, with system UI fallbacks  
**Label/Mono Font:** Cascadia Mono for paths, timestamps, and provider values

**Character:** The type is compact, direct, and desktop-native. It uses weight and spacing rather than oversized display drama.

### Hierarchy

- **Headline** (760, 25px, 1.2): selected meeting title and the strongest task heading.
- **Title** (740, 17px, 1.25): panel titles, Local AI status, settings group titles.
- **Body** (400, 14px, 1.55): summaries, transcript text, descriptions. Keep body copy under 75ch when possible.
- **Label** (720, 12px, 1.2): metadata, status labels, compact field labels. Uppercase is allowed only for very short structural labels.
- **Mono** (400, 12px, 1.45): paths, provider IDs, timestamps, and values that benefit from fixed-width rhythm.

### Named Rules

**The Product Type Rule.** Do not use display fonts, fluid hero type, decorative letter spacing, or marketing-scale headings. This is app UI.

## Elevation

Depth is mostly structural: rails, borders, and tonal surfaces establish the app shell. Shadows are nearly flat and exist only to separate primary work surfaces from rails.

### Shadow Vocabulary

- **Panel Night Shadow** (`0 18px 44px rgba(0,0,0,0.34)`): capture console and meeting detail only.
- **Control Shadow** (`0 8px 24px rgba(0,0,0,0.18)`): primary button hover only.

### Named Rules

**The Flat Rails Rule.** Navigation, meeting list, and settings rail stay flat. Only the live work surface earns lift.

**The Hard Surface Rule.** Do not pair 1px borders with wide blurry shadows. Use a hairline border, a tiny shadow, or a state color, but never decorative softness.

## Components

### Buttons

- **Shape:** squared product controls (6px radius).
- **Primary:** solid Soft Amber with Deep Space text, 48px tall for recording, 38px for ordinary actions.
- **Secondary:** white surface, Strong Divider Line border, Soft Ink text.
- **Ghost:** transparent or low-opacity Observatory Panel background with Pale Cyan text.
- **Hover / Focus:** hover subtly lifts or shifts tone; focus uses a 2px pale-cyan focus ring; active state compresses slightly for click feedback.

### Chips

- **Style:** low-opacity Observatory Panel background with Pale Cyan or Muted Starlight text and 4px to 6px radius.
- **Use:** status labels such as `summarized`, provider-generated state, and count indicators.
- **Rule:** chips are state indicators, not decorative badges.

### Cards / Containers

- **Corner Style:** 6px radius.
- **Background:** Ink Night for panels, Observatory Panel for source tiles and empty states.
- **Shadow Strategy:** only capture console and meeting detail use a tiny Work Surface Lift.
- **Border:** Divider Line for panels, Strong Divider Line for controls.
- **Internal Padding:** 17px desktop panel padding, 14px compact/mobile panel padding.

### Inputs / Fields

- **Style:** white background, Strong Divider Line border, 6px radius, 40px default height.
- **Focus:** border shifts to Pale Cyan and receives a 2px translucent cyan ring.
- **Disabled:** lower opacity, still readable.
- **Dropdowns:** use the code-native `AtlasSelect` listbox instead of native `<select>` when the expanded menu is visible in the app. The menu must inherit Night Atlas / Archive Sheet tokens and render above scrollable panels.

### Navigation

- **Style:** compact top atlas navigation on desktop and mobile.
- **Active:** low-opacity Observatory Panel fill with Pale Cyan text and hairline border.
- **Interaction:** hover changes tone, active state compresses slightly.

### Meeting Row

- **Style:** flat by default, selected row uses a low-opacity cyan wash and a full hairline border state. Do not use side stripes.
- **Content:** title, time, short summary, and real counts only.
- **Rule:** rows may truncate long titles with ellipsis, but they must not invent attendees, owners, or calendar context.
- **Archive action:** archive is a soft hide action, not deletion. Use a direct `Archive` control with explanatory tooltip copy instead of destructive color.

### Capture Console

- **Style:** lightly separated functional panel with two source tiles and a dominant recording action.
- **State:** source readiness uses small cyan dots and direct labels.
- **Motion:** panel enters with a short upward fade; recording button gives hover and active feedback.

### Observatory Shell

- **Style:** top identity/header, left archive instrument, central capture/session area, right instrument stack, and bottom local path footer.
- **Rule:** do not recreate a generic `sidebar / meetings rail / workspace / right rail` dashboard. The shell should feel like a single atlas workspace with instruments placed on it.

## Do's and Don'ts

### Do:

- **Do** keep app UI text and controls code-native.
- **Do** keep selected rows, ready states, and focus rings visually related through Pale Cyan.
- **Do** use internal scrolling inside app regions rather than page-level scrolling on desktop.
- **Do** keep controls at 6px radius and familiar desktop proportions.
- **Do** use motion for state confirmation: hover, focus, active, selected, arrival, and successful action notices.
- **Do** preserve honest empty states when backend data is missing.
- **Do** use tooltips on icon-only actions whose effect is not obvious from the icon alone.

### Don't:

- **Don't** add fake attendees, avatars, owner labels, due dates, charts, storage capacity, integrations, or analytics unless the data contract exists.
- **Don't** use beige, cream, sand, brown, purple gradients, bokeh, glassmorphism, or decorative blobs.
- **Don't** use gradients, wide drop shadows, side stripes, or pill-shaped cards to imply polish.
- **Don't** use nested cards, repeated identical card grids, or marketing hero composition.
- **Don't** introduce green as a second state language. Cyan means signal; amber means decisive action.
- **Don't** animate page-load choreography. Product UI should arrive quickly and respond to action.
- **Don't** hide focus indicators or ship controls without hover, focus, active, disabled, loading, and error affordances.
