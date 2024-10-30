# redefaulter

### Since Microsoft doesn't know what a default device means

## Config

```toml
[devices]
unify_communications_devices = true

[devices.default]
playback = "Speakers (Gaming Headset)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
playback_comms = ""
recording = "Microphone (Gaming Headset)~{0.0.1.00000000}.{xx-yy-zz-789-098}"
recording_comms = "Microphone (Gaming Headset)~{0.0.1.00000000}.{xx-yy-zz-123-098}"
```

### Windows-specific options

- `unify_communications_devices` - Any actions a profile takes towards a role, will also apply to the Communications variant of it.
  - All communications entries are ignored, higher priority files that change just communications device will be ignored.

## Profiles

Example:

Changes the default playback and recording device to the first found Bigscreen Beyond items when SteamVR's `vrserver` is running.

```toml
process_path = "vrserver.exe"
playback = "Speakers (Beyond Audio Strap)"
recording = "Microphone (Beyond)"
```

### Audio Device matching

In order of most to least generic:

Find any device with this name (regardless of numeric prefix added by Windows):

```toml
playback = "Speakers (Beyond Audio Strap)"
```

Find device with this GUID, if not, any device matching the name, regardless of prefix:

```toml
playback = "Speakers (Beyond Audio Strap)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
```

Find any device with this name (will not ignore prefix):

```toml
playback = "Speakers (3- Beyond Audio Strap)"
```

Find device with this GUID, regardless of name:

```toml
playback = "{0.0.0.00000000}.{aa-bb-cc-123-456}"
```

Find device with this name (with prefix) or GUID:

```toml
playback = "Speakers (3- Beyond Audio Strap)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
```

### Process matching

#### Currently process matching is case-sensitive, but not slash direction-sensitive (as long as they are properly escaped!)

Any instance of an application, regardless of executable's parent path:

```toml
process_path = "vrserver.exe"
```

Any instance of an application, matching the given full path:

```toml
process_path = "C:/Program Files (x86)/Steam/steam.exe"
```

### Warning for system executables!

Windows will not always properly report the process' path, however.

`notepad.exe` for example, will be reported as `C:/Windows/system32/notepad.exe` (Lowercase S!)

But other apps in that same directory (like `smartscreen.exe`) will show up with their expected Uppercase S.

> [!WARNING]
> ```toml
> process_path = "C:/Windows/System32/notepad.exe"
> ```
> May not work correctly!
