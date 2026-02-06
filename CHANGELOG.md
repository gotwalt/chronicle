# Changelog

All notable changes to Chronicle will be documented in this file.

## [0.1.1] - 2026-02-06

### Features

- Add release pipeline: CI, automated release, crates.io publishing, self-annotation ([5a3cfd7](https://github.com/gotwalt/git-chronicle/commit/5a3cfd7988b9e715494b1024616b6aee49c988c7))
- Add rapid onboarding: setup, reconfigure, backfill commands + enhanced init ([84a7fb2](https://github.com/gotwalt/git-chronicle/commit/84a7fb2717d1d568d2e2bdd052be396aecdec077))
- Add multi-language AST parsing: TypeScript, Python, Go, Java, C, C++, Ruby ([2a8a8c4](https://github.com/gotwalt/git-chronicle/commit/2a8a8c43335083f0e8a53203e095adeb6dd52228))
- Add Claude Code skills for context reading, annotation, and backfill ([75a50dc](https://github.com/gotwalt/git-chronicle/commit/75a50dc76db120d42af30b00f5e927ec45fd68e1))
- Add git chronicle show: interactive TUI explorer for annotated source ([73b7254](https://github.com/gotwalt/git-chronicle/commit/73b7254095a65ec2f868dbe1f50c7f58a94cfc52))
- Add README, CLAUDE.md, and HISTORY.md; fix corrections field in tests ([7cb1f87](https://github.com/gotwalt/git-chronicle/commit/7cb1f87616507a38ebed490f2b8890173f54fcdf))
- Add advanced query commands: deps, history, summary (Feature 08) ([dc47eec](https://github.com/gotwalt/git-chronicle/commit/dc47eec1a5a3539f250f7e3d50022a74bc5d37ef))
- add team operations — sync, export/import, doctor ([dbd437e](https://github.com/gotwalt/git-chronicle/commit/dbd437e9e9ca6e885c66109cac6fb4367626dfb6))
- Add team operations: sync, export, import, doctor (Feature 10) ([00b6a80](https://github.com/gotwalt/git-chronicle/commit/00b6a80ce3793411cd98722cafc49343ed048cc6))
- add history rewrite handling (squash synthesis, amend migration, hook installation) ([bc4563d](https://github.com/gotwalt/git-chronicle/commit/bc4563d8761daf9851013ccc99d8b4617bb42834))
- Add squash synthesis and prepare-commit-msg hook (Feature 09) ([9b33dfc](https://github.com/gotwalt/git-chronicle/commit/9b33dfcdbd0d38c5d7f783337bee647f2a4c4238))
- add annotation corrections (flag & correct commands) ([8a24f24](https://github.com/gotwalt/git-chronicle/commit/8a24f24c2fab0b1787e805db90f57a4229f6c063))
- Add read pipeline from read-agent merge (manual recovery) ([1170c01](https://github.com/gotwalt/git-chronicle/commit/1170c0148e3e49603ec890ce8f994c2e78b361a9))
- Add integration test suite for write path ([1f49bd8](https://github.com/gotwalt/git-chronicle/commit/1f49bd859b478616ff4ddf6ab3d26ce27053291a))
- Add --live flag to ultragit annotate for zero-cost stdin annotation ([ad912f0](https://github.com/gotwalt/git-chronicle/commit/ad912f0ab005e6181f42d530ae89d31e4dd2f9fa))
- Add --live flag to ultragit annotate for zero-cost stdin annotation ([847541a](https://github.com/gotwalt/git-chronicle/commit/847541a0e0aa3cbb52dbddd0ed5d23a562fbf69d))
- Add integration test suite for write path ([5c626bd](https://github.com/gotwalt/git-chronicle/commit/5c626bd150b99a3aeb6b75639b0814e2ad43fdad))
- Add --live flag to ultragit annotate for zero-cost stdin annotation ([bbddab9](https://github.com/gotwalt/git-chronicle/commit/bbddab982b1dc97219420bc6f942523af28a3a0c))
- Add live annotation integration test ([8c2fd35](https://github.com/gotwalt/git-chronicle/commit/8c2fd355323e93c5319c54ff69d9922cbe3c5f7c))

### Other Changes

- Make annotate --live input more forgiving: optional anchor, path alias, flexible constraints ([e178ab5](https://github.com/gotwalt/git-chronicle/commit/e178ab5902ca99369186c81b4a71b889fd01f103))
- Make annotate --live input more resilient with serde defaults ([ec98fa6](https://github.com/gotwalt/git-chronicle/commit/ec98fa6c3701b4ab32375669bc91d2ba39172f91))
- Make annotate --live input more resilient with serde defaults ([4a051d5](https://github.com/gotwalt/git-chronicle/commit/4a051d5b103f05be52d89c3f20f73a68208e7ea6))

### Refactoring

- Replace reqwest/tokio with ureq; drop async HTTP stack ([566a553](https://github.com/gotwalt/git-chronicle/commit/566a5539a7d5e98a78b943b0339db0564ed362c4))
