# pyes

`pyes` prefixes each input record with the probability that [TypeSafe AI's Jev model](https://typesafe.ai) answers yes to each question given, so `sort`, `head`, `awk` and `cut` rank and filter the output of any command such as `rg`.

## Install

Download from the [latest release](https://github.com/visionsofparadise/pyes/releases/latest).

| Platform      | Asset                                                                      |
| ------------- | -------------------------------------------------------------------------- |
| Windows x64   | `pyes-<version>-windows-x64.exe`, an installer                             |
| Windows arm64 | `pyes-<version>-windows-arm64.exe`, an installer                           |
| Linux x64     | `pyes-<version>-linux-amd64.deb` or `pyes-<version>-linux-x86_64.AppImage` |
| macOS arm64   | `pyes-<version>-mac-arm64.dmg`, holding `pyes.app/Contents/MacOS/pyes`     |
| macOS x64     | `pyes-<version>-mac-x64.dmg`, holding `pyes.app/Contents/MacOS/pyes`       |
| Linux arm64   | `pyes-<version>-aarch64-unknown-linux-musl`, a standalone binary           |

The Windows installer puts `pyes.exe` in `%LOCALAPPDATA%\pyes` and adds that folder to the user `PATH`. Open a new terminal afterwards. Every platform also has a standalone binary named `pyes-<version>-<target triple>`.

## Authentication

Store the API key by piping it to `pyes auth`:

```sh
echo "$KEY" | pyes auth
```

The key goes to `%APPDATA%\pyes\key` on Windows and to `$XDG_CONFIG_HOME/pyes/key` elsewhere, or `~/.config/pyes/key` when `XDG_CONFIG_HOME` is unset. On Unix the file is readable by its owner only.

`TYPESAFE_API_KEY` takes precedence over the stored key when it is set and non-empty. `TYPESAFE_BASE_URL` replaces the API base `https://api.typesafe.ai`.

## Usage

```text
pyes [-z | -d <separator>] <question>...
pyes auth
```

Each question is a yes/no question asked of every record. Records are lines by default.

| Option            | Effect                                                                                             |
| ----------------- | -------------------------------------------------------------------------------------------------- |
| `-z`              | Split records on NUL, as `git log -z` writes them.                                                 |
| `-d <separator>`  | Split records on this text, with `\0`, `\n`, `\t` and `\` unescaped. Cannot be combined with `-z`. |
| `-h`, `--help`    | Print help.                                                                                        |
| `-V`, `--version` | Print the version.                                                                                 |

### Output

```text
p1<TAB>…<TAB>pn<TAB>record
```

Each record comes back unchanged in input order, prefixed with one probability per question in the order the questions were given. Output records end in a newline for lines and in NUL for `-z` and `-d`. Empty input prints nothing. Lines drop a trailing carriage return.

Sort numerically on the first probability with `sort -rn`, or `sort -zrn` for NUL records. `cut -f<n+1>-` removes the `n` probability columns.

### Exit status

| Code | Meaning                                                                              |
| ---- | ------------------------------------------------------------------------------------ |
| `0`  | Success, including empty input, `--help` and `--version`.                            |
| `2`  | Any failure, with `pyes: <message>` on stderr: usage, missing key, API or I/O error. |

## Examples

Rank `rg` matches by how likely each is to answer the question, and keep the top five:

```sh
rg -n -i reject src | pyes "Is this the rule for how a commit records an alternative that was considered and rejected?" | sort -rn | head -5
```

Ask two questions and keep the lines where the first is likely and the second is not:

```sh
rg -n TODO src | pyes "Is this a bug?" "Is this blocked on another team?" | awk -F'\t' '$1 > 0.8 && $2 < 0.2'
```

Rank whole commit messages, which contain newlines, with NUL-separated records:

```sh
git log -z --format=%B | pyes -z "Does this commit fix a bug?" | sort -zrn | head -z -n 5 | tr '\0' '\n'
```

## License

[MIT](LICENSE)

The licences of the bundled third-party dependencies are in `THIRD-PARTY-NOTICES.txt`, published beside the installers on each release.
