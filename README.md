# redefaulter

[![Build](https://github.com/nullstalgia/redefaulter/actions/workflows/build.yml/badge.svg)](https://github.com/nullstalgia/redefaulter/actions/workflows/build.yml)

### Since Microsoft doesn't know what a default device means.

#### Redefaulter in action:

https://github.com/user-attachments/assets/06ab6596-db4c-45d7-953e-86b15a0a34b7

https://github.com/user-attachments/assets/6d256c27-b104-4c37-8c68-2213b493d137

## Features
- Lets you enforce a chosen set of Default Playback/Recording devices.
- Option to have the Communications devices *always* follow the Default audio device.
- Create Profiles to change desired devices based on what applications are running.
- Robust tray menu for configuration.
- (Experimental!) ShadowPlay Support!

## Config

```toml
[devices]
fuzzy_match_names = true
save_guid = true
unify_communications_devices = true
shadowplay_support = false

[devices.default]
playback = "Speakers (Gaming Headset)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
playback_comms = ""
recording = "Microphone (3- Gaming Headset)~{0.0.1.00000000}.{xx-yy-zz-789-098}"
recording_comms = ""
```

- `fuzzy_match_names` - When true, prefers to save and match device names generically, **always** ignoring any numeric prefix added by Windows (like `playback`'s example entry in the config above).
  - If disabled, saves and matches device names as-is (like `recording`'s entry).
  - Enabled by default, recommended to keep on.

- `save_guid` - When true, saves devices along with their OS-Given GUID (like both example entries above).
  - If disabled, saves just the device's name.
  - Enabled by default.
  - Safe to disable if you __don't__ plan to have multiple of the same device connected.

### Windows-specific options

- `unify_communications_devices` - Any actions a profile takes towards a role, will also apply to the Communications variant of it.
  - When enabled, **all** communications entries are ignored. (Any higher priority profile entries that change only communications device will be ignored.)
  - Note: Without any profiles or preferred devices set, Redefaulter will still ensure the Communications device follows the Default device!

Demo (no active profiles and no preferred devices):

https://github.com/user-attachments/assets/58f64e59-afca-41e3-89d2-863a4821bf67

### ShadowPlay Support (Experimental!)

- `shadowplay_support` - When enabled, Redefaulter will try to keep the chosen recording device for NVIDIA's ShadowPlay feature the same as the Default Recording[^1] device.

<sup>Because ShadowPlay doesn't have a "Use Windows' Default Device" option for whatever reason.</sup>

<sup>Only supports GeForce Experience. NVIDIA App uses `MessageBus`, which requires further investigation.</sup>

[^1]: Not the Recording Communications device.

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

#### In order of most to least generic:

Find any device with this name:

```toml
playback = "Speakers (Beyond Audio Strap)"
```

```toml
playback = "Speakers (3- Beyond Audio Strap)"
```

<sup>Enable `devices.fuzzy_match_names` to ignore numeric prefixes!</sup>

Find device with this GUID, if not found, try to find device by name:

```toml
playback = "Speakers (Beyond Audio Strap)~{0.0.0.00000000}.{aa-bb-cc-123-456}"
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

### (Under Construction)
