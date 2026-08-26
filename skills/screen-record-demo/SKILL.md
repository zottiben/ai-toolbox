---
name: screen-record-demo
description: Record a short screen demo (20-60s) of a running web UI, with the chrome-devtools MCP driving the page and a synthetic cursor making the interaction readable. Covers what is and is not scriptable on macOS, the choreography rules that stop a take being wasted, and how to prove the file actually captured. Use when asked for a demo video, screen recording, or walkthrough of an implementation.
---

# screen-record-demo - record a demo of a running UI

Driving the page yourself gives a repeatable, precisely paced demo. It also has
sharp edges that waste whole takes. Read the limits before promising anything.

## Know the limits before you promise

- **CleanShot X is not scriptable for recording.** Its URL scheme exposes
  screenshot commands only; recording needs a human to pick an area and press
  Record. Confirm for the installed build rather than assuming:
  `plutil -extract CFBundleURLTypes json -o - "<App>/Contents/Info.plist"` and
  `strings "<App>/Contents/MacOS/<bin>" | grep -oE '^(record-screen|start-recording|stop-recording)$'`
- **`screencapture -v` is scriptable** and built into macOS. It records the
  **whole screen**; `-R` (region) does **not** combine with `-v`.
- **It needs Screen Recording permission** for the terminal. Without it, it exits
  silently and writes no file at all.
- **CDP input does not move the real macOS pointer.** A recording of CDP-driven
  clicks shows the UI changing with nothing visibly causing it. Inject a
  synthetic cursor - see `demo-cursor.js` beside this file.
- **`ffmpeg`, `cliclick` and pyobjc/Quartz are usually absent.** So no cropping,
  no trimming, no real cursor control, no GIF conversion. Do not install them
  without asking; say what you cannot do instead.

If the ask specifically needs a real cursor, a cropped frame, or a GIF, say so
and offer to have the user record while you drive the browser.

## Procedure

0. **Test permission first**, before any setup:
   `screencapture -v -V 2 /tmp/t.mp4 >/dev/null 2>&1; sleep 5; ls -lh /tmp/t.mp4`
   No file means permission is missing. Ask for it; it cannot be granted from here.
1. **Get the app running and reachable.** Project-specific - use the project's own
   bring-up or e2e skill for this.
2. **Fill the screen with the browser** so the recording does not leak the desktop
   or other apps. Read the logical screen size from the page itself
   (`() => [screen.width, screen.height]`), then:
   ```
   osascript -e 'tell application "Google Chrome" to activate'
   osascript -e 'tell application "Google Chrome" to set bounds of front window to {0, 0, W, H}'
   ```
   Do not use Finder's desktop bounds for the screen size; that AppleScript can hang.
3. **Dismiss the automation infobar** ("Chrome is being controlled by automated
   test software"). It is browser chrome, so CDP cannot reach it. Click it through
   accessibility, then confirm:
   ```
   osascript -e 'tell application "System Events" to click at {x, y}'
   ```
   A successful hit reports `button ... of group Infobar ...`. Verify with a still:
   `screencapture -x /tmp/f.png` and read it.
4. **Position the page.** Find the real scroll container first - pages often scroll
   inside a div, so `window.scrollTo` silently does nothing. Walk up from the target
   for `overflowY: auto|scroll` with `scrollHeight > clientHeight`, then set
   `scrollTop`. Leave room below anything that opens downward, or it gets clamped
   over its own trigger.
5. **Inject the cursor** from `demo-cursor.js`. Paste its body inline into
   `evaluate_script`; the tool's `filePath` argument saves output and cannot load a
   script, and the skill directory sits outside the workspace root anyway.
6. **Write the choreography** as one async `run()` on `window.__demo`, per the rules
   below.
7. **Record and drive:**
   ```
   nohup screencapture -v -V 30 "<out>.mp4" >/dev/null 2>&1 &
   sleep 2
   ```
   then `evaluate_script` calling `window.__demo.run()` **without awaiting it**
   (awaiting blocks the tool call for the whole demo), then `sleep <secs + 4>`.
8. **Verify** - see below.
9. **Restore** anything you changed: dev servers, symlinks, window size, scroll.

## Choreography rules

- **Wrap the body in try/catch** that pushes to a log array. An uncaught throw
  ends the run silently and you get a half-length video with no error anywhere.
- **Resolve elements at click time**, never from a snapshot taken earlier. The DOM
  changes under you as panels open.
- **Never hardcode dates, months or row positions.** Read what the app is actually
  showing (a heading, an `aria-label`) and derive from it. The app's "today" is not
  your assumption, and a date that does not exist in the rendered view kills the run.
- **Move, pulse, then click.** Around 700-900ms to travel and 1200-1800ms to dwell
  after a state change, so a viewer can read what happened.
- **Type character by character**, ~90ms apart, using the native value setter plus
  an `input` event so React sees each keystroke:
  ```js
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
  setter.call(el, el.value + ch);
  el.dispatchEvent(new Event('input', { bubbles: true }));
  ```
- **Budget the time.** Sum your waits and aim to finish ~3s under the capture
  length. Overrunning truncates the ending.
- **End on a clean, complete state.** A half-finished selection makes the final
  frame look broken.

## Verify - never assume it captured

- `window.__demo.log` contains your completion marker, not a `FAILED:` entry.
- The final DOM state matches what the demo should have produced.
- **Thumbnail the video and look at it.** File size proves nothing; a black
  recording is still megabytes:
  ```
  qlmanage -t -s 1500 -o /tmp/thumb "<file>.mp4"
  ```
  then read the PNG.
- `mdls -name kMDItemDurationSeconds -name kMDItemPixelWidth "<file>.mp4"`

Report honestly that the cursor is synthetic, and say what the recording includes
(menu bar, browser chrome) so nobody is surprised when they share it.

## Pitfalls

- Recording the full screen captures **whatever else is on it**. Fill the screen
  with the browser first, and warn the user before you start.
- Repointing a shared dev-server symlink can leave PHP's realpath cache serving
  the old target for ~2 minutes, so the page silently shows another checkout's
  code. Poll a known-different URL until it flips before you record.
- `pkill -f "<generic pattern>"` kills other checkouts' dev servers too. Match on
  the absolute path, or resolve each PID's cwd with
  `lsof -p <pid> -a -d cwd -Fn`.
- Expect to redo a take. Verify after each one rather than stacking changes.
- Anything reached through `osascript` needs Accessibility permission for the
  terminal. Reads that return a value prove it is granted; a keystroke that
  silently does nothing usually means the shortcut does not apply, not that
  permission is missing.
- `tell application "Finder" to get bounds of window of desktop` can hang for
  minutes. Read `screen.width`/`screen.height` from the page instead.
