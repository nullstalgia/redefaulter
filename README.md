# redefaulter

### Since Microsoft doesn't know what a default device means.

#### Redefaulter in action:

https://github.com/user-attachments/assets/1cf8cc42-9281-49fd-9198-d92609858934

## Features
- Lets you enforce a chosen set of Default Playback/Recording devices.
- Option to have the Recording Communications device *always* follow the Default Recording device.
- Create Profiles to change desired devices based on what applications are running.
- Robust tray menu for configuration.
- (Experimental!) ShadowPlay Support!

## Config

```toml
[redefaulter]
always_save_generics = true

[devices]
unify_communications_devices = true
shadowplay_support = false

[devices.default]
playback = "Speakers (Gaming Headset)"
playback_comms = ""
recording = "Microphone (Gaming Headset)~{0.0.1.00000000}.{xx-yy-zz-789-098}"
recording_comms = ""
```

- `always_save_generics` - When true, prefers to save devices as generically as possible. (like `playback`'s entry in the config above).
  - Otherwise, saves more specific identifiers (like `recording`'s entry in the config above).
  - Enabled by default, recommended to keep on unless you have multiple of the same device connected.

### Windows-specific options

- `unify_communications_devices` - Any actions a profile takes towards a role, will also apply to the Communications variant of it.
  - When enabled, **all** communications entries are ignored. (Any higher priority profile entries that change only communications device will be ignored.)

### ShadowPlay Support (Experimental!)

- `shadowplay_support` - When enabled, Redefaulter will try to keep the chosen recording device for NVIDIA's ShadowPlay feature the same as the Default Recording[^1] device.

[^1]: (not Recording Comms)

## Profiles

- Priorities of profiles are handled by the lexicographical [sorting](https://doc.rust-lang.org/std/cmp/trait.Ord.html#lexicographical-comparison) of all profiles' filenames[^2].
  - `99-vrserver` takes precedence over `02-notepad` which takes precedence over `01-notepad`, and so on.

- If a device in a higher priority cannot be found, other lower priority active profile's devices will be used, if they are available.

- Profile filenames must end with `.toml` to be read.

[^2]: Specifically, they're sorted by a [BTreeMap](https://doc.rust-lang.org/std/collections/struct.BTreeMap.html) with the filenames as [OsString](https://doc.rust-lang.org/std/ffi/struct.OsString.html) keys.

Example of a profile's contents:

Changes the default playback and recording device to the first found Bigscreen Beyond items when SteamVR's `vrserver` is running.

```toml
process = "vrserver.exe"
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

Find device with this name (with prefix) or GUID:

```toml
playback = "Speakers (3- Beyond Audio Strap)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
```

Find device with this GUID, regardless of name:

```toml
playback = "{0.0.0.00000000}.{aa-bb-cc-123-456}"
```

### Process matching

#### Currently process matching is case-sensitive, but not slash direction-sensitive (as long as they are properly escaped!)

Any instance of an application, regardless of executable's parent path:

```toml
process = "vrserver.exe"
```

Any instance of an application, matching the given full path:

```toml
process = "C:/Program Files (x86)/Steam/steam.exe"
```

```toml
process = 'C:\Windows\System32\notepad.exe'
```

<sup>Tip: [TOML](https://toml.io/) supports unescaped backslashes in single-quote strings, aka literal strings!</sup>

If you just want to stack sets of desired devices regardless of running apps, you can set the process path to a single `*`, and it will always be active and follow the same filename priority rules.

```toml
process = "*"
```

# CLI Args

### Under Construction
