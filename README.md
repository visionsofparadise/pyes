# pyes

Rank text records by the probability of yes to a natural-language question, using [TypeSafe AI's Jev](https://typesafe.ai).

## Install

Download from the [latest release](https://github.com/visionsofparadise/pyes/releases/latest).

## Usage

```sh
pyes [-z | -d SEPARATOR] QUESTION [...]
pyes auth
```

- `QUESTION` is a yes/no question asked of every record from stdin.
- `-z` splits records on NUL; records are lines by default.
- `-d SEPARATOR` splits records on that text, with `\0`, `\n`, `\t` and `\\` unescaped.
- `auth` stores the API key read from stdin. `TYPESAFE_API_KEY` overrides it when set.

Each record prints unchanged, in input order, after one tab-separated probability per question:

```text
p1<TAB>...<TAB>pn<TAB>record
```

Rank `rg` matches and keep the top five:

```sh
rg -n -i reject src | pyes "Is this the rule for how a commit records a rejected alternative?" | sort -rn | head -5
```

Rank commit messages:

```sh
git log -z --format=%B | pyes -z "Does this commit fix a bug?" | sort -zrn | tr '\0' '\n' | head -20
```

```sh
pyes --help
pyes --version
```

## Licence

MIT
