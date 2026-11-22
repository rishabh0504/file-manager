# Task 2: Test Live Features - Comprehensive Testing Guide

## ✅ Build Status Before Testing

```bash
./VERIFY_BUILD.sh
# Expected:
#  Library: ✅ CLEAN
#  Exports: ✅ All 3 Phase 3 modules
#  Status: ✅ Ready for testing
```

---

## 🎯 Test Plan (3 Main Areas)

### 1️⃣ Keyboard Shortcuts (Phase 2)

**Location:** Explorer Mode (file browser)

| Shortcut | Expected Behavior | Verification |
|----------|------------------|---|
| **Cmd+A** | Selects all visible files | 🔲 Count shows all files selected |
| **Cmd+X** | Adds selected to movable queue | 🔲 Movable queue count increases |
| **Cmd+Delete** | Trash selected files | 🔲 Selected files move to trash |
| **Escape** | Clears search input | 🔲 Search box empties |
| **Cmd+F** | Focus search field | 🔲 Cursor in search box |

**How to Test:**
1. Open app in Explorer mode
2. Create a test folder with 5-10 files
3. Try each shortcut
4. Verify expected behavior occurs

---

### 2️⃣ Operation Tracking & Notifications (Phase 3)

**Location:** File operations (queues, moves, archive files)

#### A. Move Files via Queue

**Steps:**
1. Create a destination folder
2. Create a queue pointing to that folder
3. Add files to queue (5-10 files)
4. Submit queue to move files

**What to Verify:**
- 🔲 Toast appears top-right: "✅ Moved X file(s)"
- 🔲 Operation appears in history footer
- 🔲 Footer shows: "✅ 1 | ❌ 0" (success count)
- 🔲 Expandable history shows operation timestamp

#### B. Failed Move (Optional)

**Steps:**
1. Try to move file to read-only location
2. Operation should fail

**What to Verify:**
- 🔲 Toast appears top-right: "⚠️ 1 file(s) failed to move"
- 🔲 Failed operation recorded in history
- 🔲 Footer shows: "✅ 0 | ❌ 1" (failure count)

#### C. Archive File Operations

**Steps:**
1. Add archive files to scan
2. Select files to move
3. Execute move operation

**What to Verify:**
- 🔲 Toast appears: "✅ Moved X archive file(s)"
- 🔲 Archive operation tracked in history
- 🔲 Separate from regular file moves

---

### 3️⃣ Settings Persistence (Phase 3)

**Location:** App startup/shutdown

#### A. Settings Load on Startup

**Steps:**
1. Close app completely
2. Delete or rename: `~/.config/sortwise/settings.json`
3. Start app
4. Check default settings loaded

**What to Verify:**
- 🔲 App starts without errors
- 🔲 Default settings applied (hide_system_files: true)
- 🔲 New settings.json created at: ~/.config/sortwise/

#### B. Settings Persist After Change

**Steps:**
1. Open app
2. Find settings (future UI - for now, verify structure)
3. Close app
4. Restart app
5. Verify settings remained

**What to Verify:**
- 🔲 ~/.config/sortwise/settings.json updated
- 🔲 Settings survive app restart
- 🔲 No errors loading/saving

---

## 📊 Operation History Testing

### Full History Display

**What to Verify:**
1. 🔲 Footer shows "📊 Operations" when history exists
2. 🔲 Compact summary: "✅ X | ❌ Y"
3. 🔲 Expandable section shows recent ops
4. 🔲 Each op shows: time, type, files, status
5. 🔲 Last 10 ops displayed (more hidden)
6. 🔲 History persists within session
7. 🔲 Clears on app restart (expected behavior)

**Example Output:**
```
📊 Operations
✅ 15 | ❌ 2

Recent Operations (Expanded):
  ✅ 14:32:01 Move file1.txt → Archive
  ✅ 14:31:45 Move folder_batch → Downloads
  ❌ 14:31:40 Move readonly.txt (Failed to move)
  ... and 12 more
```

---

## 🔧 Verification Checklist

### Before Testing
- [ ] Run `cargo build --lib` → ✅ CLEAN
- [ ] Run `./VERIFY_BUILD.sh` → ✅ All pass
- [ ] Review recent commits: `git log --oneline -5`

### During Testing

**Phase 2 Keyboard:**
- [ ] Test all 5 shortcuts
- [ ] Each responds immediately
- [ ] Visual feedback shows in UI

**Phase 3 Operations:**
- [ ] Move operations show toast notifications
- [ ] Notifications disappear after 3-7 seconds
- [ ] Operation history footer updates
- [ ] Multiple operations accumulate

**Phase 3 Settings:**
- [ ] Settings file created: ~/.config/sortwise/settings.json
- [ ] No errors on startup
- [ ] No errors on shutdown

### After Testing
- [ ] All features working as expected
- [ ] No regressions in existing features
- [ ] No crash on file operations
- [ ] No crash on app shutdown

---

## 🐛 Troubleshooting

**Notifications don't appear:**
- Verify `self.notifications.render(ctx)` in app.rs line ~1613
- Check if notifications.rs compiles cleanly
- Ensure NotificationManager initialized in Default impl

**Operation history empty:**
- Verify poll_queue_submit_result() calls app.operation_history.add()
- Check archive_ops.rs also has tracking
- Confirm operations are actually completing

**Settings not persisting:**
- Check ~/.config/sortwise/settings.json exists
- Verify AppSettings::load() called in Default impl
- Ensure Drop impl saves on shutdown

**Keyboard shortcuts not working:**
- Verify handle_explorer_keys() called in explorer_main_panel (line ~197)
- Check keyboard.rs compiles and exports correctly
- Test with Cmd key, not Ctrl (if on Windows: Ctrl)

---

## 📈 Expected Performance

| Feature | Expected Behavior |
|---------|--|
| Keyboard Response | Instant (<50ms) |
| Toast Display | Immediate appearance in corner |
| Toast Duration | 3-7 seconds based on level |
| Settings Save | <100ms |
| Settings Load | <100ms |
| Operation Record | <1ms per operation |

---

## ✅ Success Criteria

All tests pass when:

1. ✅ All 5 keyboard shortcuts work in Explorer mode
2. ✅ Toasts appear after move operations
3. ✅ Operation history shows all recent moves
4. ✅ Success/failure tracked correctly
5. ✅ Settings file exists: ~/.config/sortwise/settings.json
6. ✅ No app crashes during testing
7. ✅ No compiler errors/warnings when rebuilding

---

## 🎉 Testing Complete When:

- [ ] Keyboard shortcuts: **PASS**
- [ ] Operation tracking: **PASS**
- [ ] Notifications: **PASS**
- [ ] Settings persistence: **PASS**
- [ ] Operation history UI: **PASS**
- [ ] No regressions: **PASS**

**Result:** ✅ All Phases 1-3 Features Working & Tested

---

## 📝 Notes

- Keyboard shortcuts work in **Explorer mode only** (currently)
- Operation history shows in **explorer footer** (collapsible)
- Settings saved to **~/.config/sortwise/settings.json** (auto-created)
- Toast notifications appear in **top-right corner**
- Operation history persists for **current session only** (by design)

---

**Ready to Test?** Run app and follow verification checklist above!
