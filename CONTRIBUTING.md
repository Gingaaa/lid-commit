# Contributing to LidCommit

Thank you for your interest in contributing to **LidCommit**! 🚀

Projects like Next.js and shadcn/ui thrive because of dedicated contributors. To ensure the codebase remains high quality, legally protected, and pleasant to work on, please take a moment to review this guide before submitting issues or pull requests.

---

## 📜 Intellectual Property & Copyright Notice

- **Original Author & Maintainer**: Anshu Kumar ([@Gingaaa](https://github.com/Gingaaa))
- **License**: [MIT License](LICENSE)
- **Copyright**: Copyright (c) 2026 Anshu Kumar. All rights reserved.

### Contributor Assignment & Licensing Rules
1. **No Plagiarized Code**: All code submitted via Pull Requests must be your original work or come from an open source source with a compatible license that you have rights to submit.
2. **License Grant**: By submitting a Pull Request to LidCommit, you agree that your contributions will be licensed under the project's [MIT License](LICENSE), with Anshu Kumar retaining project ownership and maintainer rights.
3. **Trademarks & Branding**: The name "LidCommit", the logo, and official product assets belong to Anshu Kumar and cannot be used in confusingly similar competing products.

---

## 🛠️ Development Setup

### 1. Prerequisites
- **Git** (latest)
- **Rust toolchain** (v1.75+): Install via [rustup.rs](https://rustup.rs/)
- For Windows GUI compilation: C++ Build Tools (MSVC)
- For Linux: `libdbus-1-dev`, `libssl-dev`, `pkg-config`

### 2. Fork and Clone
```bash
# 1. Fork the repository on GitHub
# 2. Clone your fork locally
git clone https://github.com/<your-username>/lid-commit.git
cd lid-commit

# 3. Add upstream remote
git remote add upstream https://github.com/Gingaaa/lid-commit.git
```

### 3. Local Build & Test
```bash
# Compile and run debug build
cargo run

# Run with headless flag (testing CLI mode)
cargo run -- --headless

# Run tests
cargo test

# Check code formatting and lints
cargo fmt --check
cargo clippy -- -D warnings
```

---

## 🌿 Git Workflow & Branches

We follow standard trunk-based GitHub Flow:

1. **`main`** is our production-ready, release branch.
2. Always create a feature or bugfix branch off `main`:
   ```bash
   git checkout -b feat/your-feature-name
   # or
   git checkout -b fix/issue-description
   ```
3. Keep pull requests focused on a single change. Do not bundle unrelated refactors into one PR.

---

## 📝 Conventional Commits

We follow the standard [Conventional Commits](https://www.conventionalcommits.org/) specification used by Next.js and Vercel projects:

| Type | When to use | Example |
|---|---|---|
| `feat` | New user-facing feature | `feat(ui): add system tray menu shortcut for pause` |
| `fix` | Bug fix | `fix(daemon): prevent duplicate commit triggers on sleep wake` |
| `docs` | Documentation changes | `docs: add troubleshooting section for Linux power profiles` |
| `refactor` | Code restructuring without feature changes | `refactor(detector): simplify IDE window process handle search` |
| `perf` | Performance optimization | `perf(diff): optimize git status inspection on large repos` |
| `test` | Adding or updating tests | `test(git): add mock unit tests for dirty tree detection` |
| `chore` | Build, dependency, or CI updates | `chore(deps): bump wry to 0.57.1` |

---

## 🚀 Submitting a Pull Request (PR)

1. **Check existing issues/PRs** first to make sure someone isn't already working on the same thing.
2. If proposing a major feature or architectural rewrite, please **open an Issue or RFC discussion first** so we can agree on the design before you write thousands of lines of code.
3. Make sure your branch builds cleanly:
   ```bash
   cargo build --release
   cargo test
   ```
4. Push your branch to your fork and submit a PR to `Gingaaa/lid-commit:main`.
5. Fill out the Pull Request template completely (explain what changed, why, and how to test it).

---

## 🤝 Code of Conduct

We are committed to providing a welcoming, inclusive, and harassment-free environment for everyone. Please treat all contributors and maintainers with respect, empathy, and professional courtesy.

For questions or security concerns, contact: **anshusee22@gmail.com**.
