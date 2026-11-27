# Design recipe

- Medium: HTML mockups on the Blocks design tokens, one folder per design: `docs/design/<slug>/`.
- Sizes: 1440 and 390 wide, light by default unless the issue asks for dark.
- Render: a `render.sh` in the design folder drives Playwright's cached headless Chromium and writes one PNG per
  state and size into the same folder.
- README: the folder's `README.md` maps every PNG to the decision it shows and lists what impl takes from it.
- The approved restart cockpit is the example: [docs/design/restart/](../docs/design/restart/README.md).
