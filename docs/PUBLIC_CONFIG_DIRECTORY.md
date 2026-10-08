# Explicit app-state directory (S5 investigation support)

Scope: S5-I2-C investigation support, R16/R18/R21/R22, local AT-063/065/066/077/086/090/094/097 regressions. Allowed changes are GUI startup arguments, app-state path selection and focused tests. This does not introduce a manufacturing CLI or change import, queue, cancellation, rendering, project schema, acceptance thresholds or platform requirements.

Start the GUI executable with exactly two arguments:

```text
RCam --config-dir <absolute exclusive directory>
```

The parent directory must already exist. The selected directory may be absent, empty, or contain only synthetic `preferences.json` and/or `shortcuts.json` seed files. RCam creates the directory with private permissions on Unix, writes an ownership marker and holds an operating-system advisory lock until process exit. A directory previously initialized by this option can be reused after that process exits. Each simultaneous RCam instance needs its own directory; the same directory cannot be shared by cooperating instances.

All app state goes under the selected directory:

| State | Relative path |
| --- | --- |
| Preferences | `preferences.json`, including `preferences.json.tmp` |
| Shortcuts | `shortcuts.json`, `shortcuts.json.lock`, `.rcam-shortcuts-*.tmp` |
| Recovery | `recovery/`, including snapshot/metadata temporary files and explicit recovery deletion |
| Diagnostics | `logs/`, including rotating runtime/operation logs and `logs/crashes/` |
| Directory ownership | `.rcam-config-owner`, `.rcam-config.lock` |

Arguments are validated before preferences, recovery discovery, diagnostics or GUI startup. Missing, relative, duplicate or conflicting arguments fail startup. The equals form is unsupported and rejected, including non-UTF-8 arguments. Explicit mode rejects internal native state-directory environment overrides, overlaps with the default RCam state directories, unrelated existing directory contents, symbolic links/reparse points, wrong entry types and unwritable state. Unix additionally rejects hard-linked files. Existing managed trees are inspected with entry and depth bounds. Diagnostics startup failure is also fatal in explicit mode. There is no fallback to default directories.

No argument preserves the existing default paths and error handling, including internal evidence overrides. HOME, APPDATA and global settings are never changed. Opening or saving a project still accesses the explicitly chosen project path; this option controls app state, not all application file access. Preference seeds can contain recent-project paths, and recovery metadata can refer to an original project. Native investigation must use fresh directories or synthetic seeds only, without copying user preferences or recovery data.

The ownership marker and advisory lock coordinate RCam instances. They are not protection against another program or the same user maliciously replacing entries while RCam runs. The directory and its ancestors must be controlled by the launcher; keep them untouched during the run. Windows hard-link isolation has not been verified; Windows native acceptance remains deferred. A failed explicit startup may leave ownership/lock files or managed directories inside a valid dedicated directory, but never writes the default RCam directories. Invalid unrelated existing directories are rejected before creating the lock file.

For Mac investigation, invoke the executable directly with the option, retain the exact binary/source identity and use a different fresh directory per run. This is a new experimental variable. The earlier native heap-corruption failure remains failed; app-state isolation does not establish or repair its cause. Mac Test/Release, Metal and native reproduction remain required under their current protocol.
