# Studio design contract

Purpose: connect a real headphone and make its confirmed sound settings understandable. First launch has real discovery and a deliberate, separately labeled studio preview. Preview is an interaction demonstration; its ten-band EQ and switches do not assert model capabilities.

Direction: tactile audio studio, with restrained industrial detail. Anchor: authentic Sony product renders matched to the selected model, oversized condensed model typography, engraved frequency guides and one amber signal color. The user explicitly requested original headphone product art; invented illustrations were removed. Subjective DFII: impact 4, fit 5, feasibility 5, performance 4, consistency risk 2 = 16; this is not validation.

Typography: system condensed display stack for immediate native rendering, system sans for controls and monospace for frequency/data labels. No runtime remote fonts or assets: official product renders are bundled for offline display, with provenance and rights notes in public/headphones/SOURCES.md. Tokens govern charcoal/ivory surfaces, amber, text, lines and all spacing. Density: an anchored device rail and a generous sound workspace; no generic metrics dashboard.

Required flows: actual discovery/connection state; opt-in preview; preview model switching, noise/ambient, EQ presets and per-band edits, custom preset save/delete/reset, device switches, coverage inspection, native diagnostics, theme/reduced motion preferences. Unavailable controls explain the missing validation/capability. Native errors never create local success. Closing behavior is described in app settings.

Motion: one Anime.js entrance sequence and short scene reveals; direct CSS control feedback and EQ curve transitions. Hidden pages retain no perpetual loops. Reduced motion bypasses choreography, while status changes remain visible. State updates never wait for animation.

Responsive behavior: rail becomes a top strip below 900px; paired panels stack below 1100px; frequency editor scrolls inside its panel at narrow widths; 390px page has no overall horizontal overflow. Focus uses a high-contrast amber outline. Every range has a name/value; all menus and actions use native controls.

Acceptance: native launch does not invent devices/battery/firmware; preview label persists across every screen; model changes preserve independent preview settings; EQ reset/save works after reload; connection failures remain actionable; keyboard navigation works; both themes and reduced-motion render clearly. Hardware capabilities require independent runtime evidence.
