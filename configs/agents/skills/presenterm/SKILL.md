---
name: presenterm
description: Author and refine presenterm Markdown terminal presentations, themes, diagrams, layouts, and PDF or HTML exports. Use for presenterm decks, not PowerPoint or Google Slides.
---

# Presenterm Skill

Preserve presenterm as the output format. Check the installed version and available renderers before choosing features. Read the Design Rules before authoring and consult Slide Recipes in the reference when choosing a composition. Use hot reload when an interactive terminal is available; otherwise use supported validation or export commands and report any visual checks you could not perform. Inspect executable snippets before running validations or exports that execute code.

## Core Mental Model

A presenterm deck is **one markdown file**. There is no slide object model, no XML, no HTML layout engine.
Everything that vanilla markdown cannot express is expressed with **comment commands** — single-line HTML
comments that presenterm interprets as directives:

```markdown
<!-- end_slide -->
<!-- pause -->
<!-- column_layout: [2, 1] -->
```

Three consequences that shape how you write:

1. **A single-line HTML comment is always a command.** `<!-- remember to mention X -->` is an *error*, not a
   comment. Use `<!-- // note -->` or `<!-- comment: note -->` for real comments, or set `command_prefix`.
2. **Layout is columns only.** No divs, no absolute positioning. Composition = column layouts + alignment +
   explicit newlines. Learn to think in column units.
3. **Hot reload is the workflow.** Running `presenterm deck.md` (without `-p`) reloads on save *and jumps to
   the slide you edited*. Keep it running in a split when an interactive terminal is available.

## Design Rules

These are what separate a deck that reads well from a wall of markdown. Apply them by default.

**Structure**

1. **One idea per slide.** If a slide needs two setext titles, it is two slides.
2. **Every content slide gets a setext title.** Titles are styled, indexed in the `<c-p>` modal, and give the
   audience a place to re-anchor. An untitled slide is a lost slide.
3. **Use section dividers.** `jump_to_middle` + a title alone, every 4–6 slides. They cost nothing and give
   the deck a visible skeleton.
4. **Open with an agenda, close with a takeaway slide** whose title is the one sentence you want remembered.

**Density**

5. **Six lines of body text per slide, maximum.** Terminal real estate is small and unforgiving.
6. **Bullets are fragments, not sentences.** Three to five words. The prose is what you say out loud.
7. **Code snippets: 15 lines maximum.** Longer means `file` snippets with `start_line`/`end_line`, or split
   across slides with dynamic highlighting.
8. **Never paste a full stack trace or log dump.** Excerpt the three lines that matter.

**Rhythm**

9. **Reveal progressively when order matters.** `pause` between the problem and the solution;
   `incremental_lists: true` for a list you will narrate item by item. Do *not* make every list incremental —
   that turns navigation into a chore and hides the shape of the slide.
10. **Use dynamic code highlighting `{1-3|5-9|all}` for any walkthrough.** It is the highest-value feature in
    the tool for technical content.
11. **`n` / `p` skip pauses.** Rely on this when you need to jump around during Q&A.

**Visuals**

12. **Pair narrative with a visual using `[2, 1]`.** Text left, diagram or code right. This is the workhorse
    layout for technical decks.
13. **Center sparse slides with `[1, 3, 1]`.** A short list centered in the middle 60% looks deliberate;
    the same list flush-left in a wide terminal looks abandoned.
14. **Always set image and diagram widths explicitly.** 50–70% is the usual range.
15. **One diagram per slide.** Two diagrams means neither gets read.
16. **Use `new_lines` to breathe.** Whitespace is the cheapest design element available.

**Consistency**

17. **Pick one theme and one accent color; define both in a palette.** Reference `p:accent` everywhere
    rather than hardcoding hex, so a single edit restyles the deck.
18. **Keep the footer minimal.** `{current_slide} / {total_slides}` on the right, or a `progress_bar`.
    A busy footer competes with content on every single slide.
19. **Run `--validate-overflows`** at the terminal size you'll present at. Fix overflows by cutting content,
    not by shrinking margins.
20. **Present at a known size.** Set `defaults.max_columns: 120` so a wide terminal doesn't stretch lines to
    unreadable widths.

## Authoring Workflow

1. Scaffold the file with front matter, then an agenda slide, then one `end_slide`-terminated stub per point
   you intend to make. Get the *outline* right before writing any content.
2. When an interactive terminal is available, open `presenterm deck.md` in a split for hot reload. Otherwise, use available validation or export commands.
3. Write content slide by slide and inspect the rendered output when available.
4. Add visuals last. Diagrams and layouts are cheap to add to a good outline and cannot rescue a bad one.
5. Validate before presenting:

```bash
presenterm --validate-overflows deck.md          # at your presentation terminal size
presenterm --validate-snippets -x deck.md        # every exec snippet actually runs
presenterm -p deck.md                            # full dry run in presentation mode
```

6. Export a PDF backup when requested or useful for the delivery context.

## Gotchas

- **A single-line HTML comment that isn't a valid command is a hard error.** Use `<!-- // ... -->`,
  `<!-- comment: ... -->`, a multi-line comment, or set `options.command_prefix`.
- **A multi-line comment cannot contain a command.** `<!--\npause\n-->` does nothing.
- **`---` is ambiguous.** It is a setext underline, a thematic break, and (with `end_slide_shorthand`) a slide
  delimiter. Underline titles with `===` and delimit slides with `<!-- end_slide -->`.
- **Remote images are unsupported by design.** Vendor assets into the deck directory.
- **`include:` rewrites relative paths** to be relative to the included file, not the top-level deck.
- **`font_size` is kitty-only** (≥ 0.40) and is silently ignored elsewhere, including ghostty. Do not build a
  layout that depends on it.
- **Mermaid/D2/typst render at load time**, so a diagram-heavy deck has a visibly slow startup. Raise
  `snippet.render.threads`.
- **`+width` on a low-`scale` diagram produces blur.** Fix `mermaid.scale` / `d2.scale` first, then trim.
- **`+width` requires a literal `+render`.** `auto_render_languages` does not satisfy it — the check runs at
  parse time and fails the deck with "attribute width can only be set in +render blocks".
- **Template footers error on unknown or unset variables.** `{date}` with no `date:` in front matter fails the
  whole deck. Escape literal braces as `{{...}}`.
- **`author` and `authors` together is an error.** Pick one.
- **`-X` auto-executes code at load.** Never pass it to a deck you did not write.
- **tmux needs `allow-passthrough`** for any image or rendered diagram to appear.

## Detailed reference

Consult [references/guide.md](references/guide.md) for the following topics. Search for the relevant heading and read that section as needed:

- Toolchain
- CLI
- Deck Skeleton
- Slide Boundaries
- Comment Commands
- Layouts — the main composition tool
- Images
- Diagrams
- Code
- Themes
- Slide Recipes
- Config File
- Export
