# Running Sortwise

This guide provides instructions on how to run, test, and package the Sortwise application across different platforms and modes.

## 📋 Prerequisites

Before you begin, ensure you have the following installed:

- **Rust Toolchain**: Install via [rustup.rs](https://rustup.rs/).
- **C Compiler**: Required for building some dependencies (e.g., `Xcode Command Line Tools` on macOS).

---

## 🚀 Running the Application

### 🖥️ GUI Version (Desktop)
The default mode runs the graphical user interface built with `egui`.

```bash
cargo run
```

### ⌨️ CLI Version
The project includes a powerful command-line interface for headless operations. This requires the `cli` feature.

```bash
cargo run --bin sortwise-cli --features cli
```

---

## 📦 Building & Packaging

### 🍎 macOS Bundle (.app)
To create a native macOS application bundle:

1. **Make the script executable**:
   ```bash
   chmod +x scripts/build-macos-app.sh
   ```
2. **Run the build script**:
   ```bash
   ./scripts/build-macos-app.sh
   ```
   *Note: Use `./scripts/build-macos-app.sh --sign-adhoc` if you encounter Gatekeeper issues.*

3. **Output**: The application will be located at `dist/Sortwise.app`.

### 🪟 Windows Setup (.exe)
From a PowerShell terminal:

```powershell
.\scripts\build-windows.ps1
```

---

## 🧪 Testing & Quality

To ensure everything is working correctly, you can run the test suite:

### Run Unit Tests
```bash
cargo test
```

### Run Benchmarks
```bash
cargo bench
```

---

## 📂 Project Structure

- **`src/main.rs`**: Entry point for the GUI application.
- **`src/bin/cli.rs`**: Entry point for the CLI tool.
- **`src/lib.rs`**: Core logic and file management engine.
- **`src/ui/`**: UI components and styling.
- **`scripts/`**: Automation scripts for cross-platform builds.

## Visual review (developer)

`sortwise --snapshot <out.png> [screen]` renders one screen (`results`, `scan`, `explorer`, `staging`, `queues`) with demo data to a PNG and exits. It uses egui's own frame capture, so it needs no screen-recording permission.
