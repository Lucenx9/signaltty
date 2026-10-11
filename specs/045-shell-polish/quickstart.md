# Verification

Run `scripts/verify.sh doctor`, then `scripts/verify.sh full`.
Native PNGs are in the run's screenshots directory. The empty-state scene test
exports via SIGNALTTY_UI_EVIDENCE; it uses isolated GTK windows and no production
socket. Inspect desktop/narrow/large light/dark screenshots, selected rows,
working vs approval, header toggle and terminal edges. Real screen-reader,
IME and desktop high-contrast checks must be reported separately.
