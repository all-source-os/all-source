# Browser observations

All data below are synthetic. Product data routes use the actual Query Service
and temporary Core. Browser controls operated the isolated Next production build.

- Desktop: 1280-pixel viewport, document scroll width 1280. Comparison history
  appears in two columns. Keyboard Tab from the review button reaches the first
  evidence sequence summary with a visible focus ring.
- Mobile: 390 by 844, document scroll width 390. Baseline/candidate stack. Hashes
  wrap. Final controls measure 16px text and at least 44px height; the long review
  button wraps to 66px instead of overflowing its card.
- Inspection returned revision 7, one change and one attempt for the third
  recorded run. Consent started unchecked and sharing was disabled. Explicit
  consent followed by sharing added the source to persisted references.
- The prepared comparison exposed the original run IDs, exact digest and hashes,
  first divergence at change 1, accepted/pass baseline and reverted/fail candidate.
  The independent review banner remained unapproved with no execution.
- Revoking the baseline source removed the prior report. Opening it again showed
  an unavailable receipt with no baseline/candidate report content.
- Connection discovery failure showed an alert. A misleading empty-state message
  seen during the expired test-service credential incident was fixed and covered
  by a regression test.
- Synthetic fixture lifecycle and final reload checks are recorded in
  `browser-fixture.txt`. Screenshots were visually inspected in the task output;
  these notes do not claim an exported screenshot file or a screen-reader session.
