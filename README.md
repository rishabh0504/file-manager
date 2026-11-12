# Packaging — installable app (macOS & Windows)

## macOS (Applications folder)

1. On a Mac, from the repo root:
   ```bash
   chmod +x scripts/build-macos-app.sh
   ./scripts/build-macos-app.sh
   ```
2. Optional ad-hoc signature (helps some Gatekeeper cases; **not** notarization):
   ```bash
   ./scripts/build-macos-app.sh --sign-adhoc
   ```
3. Output: **`dist/Sortwise.app`**
4. Install: drag **`Sortwise.app`** into **`/Applications`**.
5. First launch: if macOS says the app can’t be opened, **right‑click → Open** once, or allow in **System Settings → Privacy & Security**.

**Universal binary (Intel + Apple Silicon):** build on each architecture and `lipo` the two `sortwise` binaries, then replace `Contents/MacOS/sortwise` before signing. Not automated in the default script.

**Distribution:** for public release, use Apple Developer ID signing + notarization (not covered here).

---

## Windows

### Folder install (no extra tools)

From PowerShell in the repo root (with Rust toolchain):

```powershell
.\scripts\build-windows.ps1
```

Output: **`dist/Sortwise-Windows-v0.1.0/`** containing `sortwise.exe` and `README.txt`. Copy that folder to e.g. `C:\Program Files\Sortwise\` and run the `.exe`.

### Installer (.exe setup)

1. Install [Inno Setup 6](https://jrsoftware.org/isinfo.php).
2. Build release, then either:
   ```powershell
   .\scripts\build-windows.ps1 -Installer
   ```
   or manually:
   ```text
   "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" /DMyAppVersion=0.1.0 packaging\windows\Sortwise.iss
   ```
3. Output: **`dist/Sortwise-Setup-<version>.exe`**

SmartScreen may warn until the binary is **code-signed** with a commercial certificate.

---

## Bump version

Update `version` in the root **`Cargo.toml`**, then rebuild. The macOS script injects that version into `Info.plist`; the Windows script and Inno define use it when you pass `/DMyAppVersion=…` or use `-Installer`.
