# keepy-uppy — project knowledge

2D landscape arcade game ("keepy-uppy" / balloon-bounce): a cat bounces a
balloon, dodges and catches birds, and chases a timed high score with a
compounding multiplier, achievements, and per-scene high-score tables.
**Ported from Godot to Unity** — several conventions are carried over verbatim
(see Hard rules).

**Stack:** Unity **6000.3.11f1** (Unity 6.3) · **built-in** render pipeline ·
**legacy** `UnityEngine.Input` · C#, global namespace · PlayerPrefs persistence.
Targets: Windows/macOS/Linux (Steam + direct), WebGL (itch/web), Android (AAB), iOS.

**Layout**
- `Assets/Scenes/Main.unity` — the ONLY build scene. Near-empty: a Main Camera +
  a "Setup" GameObject running `SceneSetup`.
- `Assets/Scripts/` — all game code, flat, global namespace. Runtime asmdef
  `KeepyUppy.Runtime` (references only `Unity.ugui`).
  - `SceneSetup.cs` — **entry point**; builds the entire game (managers, UI,
    colliders, walls, balloon) in `Start()`, then destroys itself. Read first.
  - Static-`Instance` singleton managers: `GameManager` (state machine),
    `ScoreManager`, `HighScoreManager` (+ hardcoded `AllAchievements`),
    `AudioManager`, `SteamManager` (stub).
  - `DifficultyConfig.cs` — **static** config (not a MonoBehaviour); call
    `EnsureInitialized()` before reading.
  - `SpriteGenerators/` — procedural pixel-art; **runtime** code (inside the
    runtime asmdef), not editor tooling. UI is 100% code-generated (no prefabs).
- `Assets/Editor/` — `PlatformBuilder.cs`, `WebGLBuilder.cs` (no asmdef →
  `Assembly-CSharp-Editor`).
- `Assets/Tests/{EditMode,PlayMode}/` — Unity Test Framework (NUnit), separate asmdefs.

**Commands**
- WebGL build (local): `./build-webgl.sh` (auto-detects the pinned editor;
  override with `UNITY_EDITOR=/path`). Headless `-executeMethod
  WebGLBuilder.BuildHeadless`; output `Builds/WebGL/index.html`.
- Other targets: editor `Build/*` menu, or `-executeMethod
  PlatformBuilder.BuildWindows|BuildMacOS|BuildLinux|BuildWebGL|BuildAndroid|BuildiOS`
  (only ever builds `Main.unity`).
- Tests: editor Test Runner, or `-runTests` (EditMode + PlayMode). CI is the
  source of truth.
- CI: `ci.yml` (push/PR → `unity-test-runner@v4` all tests + WebGL
  build-validation, `Library/` cache, `lfs: true`); `release.yml` (tag `v*` →
  tests → all-platform builds → GitHub release). Secrets:
  `UNITY_LICENSE/EMAIL/PASSWORD/SERIAL`, `ANDROID_KEYSTORE_*`.
- Unity `6000.3.11f1` is pinned in `ProjectVersion.txt` and every workflow —
  match it exactly.

## Hard rules

This is a Godot→Unity port with heavy code-gen — most Unity muscle memory is
wrong here. A fresh model gets these wrong.

### 1. Build objects in `SceneSetup`, not the scene
`Main.unity` is intentionally near-empty. Managers, UI, walls, the balloon — all
created in `SceneSetup.Start()` and wired onto `GameManager` there. Add a
system/object by registering it in `SceneSetup`, not by editing the scene. There
are **no prefabs**: the "balloon prefab" is a runtime `GameObject` cloned via
`Instantiate`.

### 2. Don't modernise the engine setup
Built-in render pipeline (no URP in `manifest.json`) and legacy
`UnityEngine.Input` (`activeInputHandler: 2`). Do **not** add URP or the new
Input System — both are absent by design.

### 3. What doesn't exist here (don't reach for it)
No namespaces (global, despite the asmdef), no prefabs, no ScriptableObjects, no
save files, no server. Config is the **static** `DifficultyConfig`; achievements
are a **hardcoded static list** `HighScoreManager.AllAchievements` (add one = add
a lambda). Persistence is **PlayerPrefs only** — keys `high_scores` (JSON top-10),
`achievements_unlocked` (csv ids), `difficulty_tier` (0–2).

### 4. The Godot-derived magic numbers are intentional
`Physics2D.gravity=(0,-980)`, camera `orthographicSize=437`, `pixelsPerUnit=1`,
and a Y-flipping `PixelArtUtils.SetPixel` (Godot y-down → Unity y-up) make world
coords huge (viewport ±437, `FloorY=-427`). They're interdependent — do not
"fix" them.

### 5. Procedural sprite conventions
Textures: `RGBA32`, `FilterMode.Point`, `TextureWrapMode.Clamp`. Always call
`PixelArtUtils.Finalise` (runs `BleedEdges` to kill black fringing) before
`Apply`. Sprites are `FullRect` with 1px extrude. `LoadIcon("UI/Icons/{name}")`
forces point filtering and does not copy pixels (imported textures may be
non-readable).

### 6. Editor code stays in `Assets/Editor/`
`KeepyUppy.Runtime` compiles for all platforms, so anything referencing
`UnityEditor` must live under `Assets/Editor/` (or an editor-only asmdef) or it
breaks WebGL/player builds. Conversely, `SpriteGenerators/` is deliberately
**runtime**, not editor tooling.

### 7. Steam is a stub — keep it guarded
Every Steamworks call sits behind `#if STEAMWORKS_ENABLED`, which is **not
defined**; there's no Steamworks package and **no `steam_appid.txt`**; the App ID
is a TODO. `UnlockAchievement`/`SubmitScore` are no-ops today. To enable Steam:
add the package, define the symbol, set the app id, add `steam_appid.txt` — and
keep calls behind the define + an `Initialized` guard so WebGL/mobile builds still run.

### 8. Resources load by string path — folder names are load-bearing
`Resources.Load` couples to folder names: `Cat1..Cat6/{Idle,Walk}`,
`Bird1..Bird5/Fly`, `Background{,1,2}`, `Audio/{SFX,Music}`, `UI/{Icons,Packs,Panels}`.
Renaming a Resources folder silently breaks loads at **runtime** (no compile
error). `.meta` files carry the guids the scene wiring depends on — never move or
delete an asset without its `.meta`.

### 9. Ignore the non-source clutter
- `.software-teams/team/worktrees/*` are 7 git worktrees each duplicating
  `Assets/Scripts` — greps return every file ~8×. They're untracked; **only edit
  the top-level `Assets/`**. (`.claude/` is agent-framework docs, not game code.)
- Committed `*-test.log`, `TestResults-*.xml`, `EditModeResults.xml`, `Unity_*.alf`
  are stale snapshots — the workflows are the source of truth, not these.
- `.gitattributes` LFS is **not** actually configured, so image/audio assets are
  plain git blobs today and CI's `lfs: true` is a no-op. Introducing large
  binaries with real LFS is a genuine migration, not a given.
