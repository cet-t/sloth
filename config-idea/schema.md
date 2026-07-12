# レイアウト設定ファイル仕様 (TOML)

`sloth-parser` が読み込む TOML レイアウト定義の仕様。JSON も同等の構造で書ける（キー名・型は同一）。

## トップレベル構造

```toml
[meta]
name = "layout-name"      # 必須
keyboard = "us"            # "us" | "jis" 省略時は "us"
author = "..."            # 任意
version = "..."           # 任意

[layers.<name>]
...                       # 層定義

[combos]                  # または [combos.<trigger>]
...                       # 同時押し

[sequences]
...                       # 順押し

[states]
...                       # 状態 → 層の選択
```

## meta

| キー       | 型            | 必須 | 説明                  |
| ---------- | ------------- | ---- | --------------------- |
| `name`     | string        | ○    | レイアウト名          |
| `keyboard` | "us" \| "jis" | -    | 物理配列（既定 `us`） |
| `author`   | string        | -    | 作成者                |
| `version`  | string        | -    | バージョン            |

## layers

`[layers.<name>]` で名前付き層を定義。単打（基本層）は `base` という名前で書くのが慣例。

### grid（位置指定）

`grid` は行×列の文字列配列。各セルは「その物理位置のキーを押したときの出力」。
行は上から:

- row0 = 数字段（us は先頭に `` ` `` を含む）
- row1 = q 段（Q,W,E…）
- row2 = a 段（A,S,D…）
- row3 = z 段（Z,X,C…）

出力が空文字 `""` のセルは無視される。

```toml
[layers.base]
grid = [
  ["`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "+"],
  ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]", "\\"],
  ["a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'"],
  ["z", "x", "c", "v", "b", "n", "m", ",", ".", "/"],
]
```

### inherit + override（差分指定）

`inherit` で別層の有効マップを受け継ぎ、`override` で上書きする。

```toml
[layers.shift]
inherit = "base"

[layers.shift.override]
grid = [                 # 位置指定でも…
  ["~", "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+"],
  ["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "{", "}", "|"],
  ["A", "S", "D", "F", "G", "H", "J", "K", "L", ":", "\""],
  ["Z", "X", "C", "V", "B", "N", "M", "<", ">", "?"],
]
"q" = "Q"                # 名前指定でも（併用可）
```

- `override` は **名前付きマップ**（`"q" = "Q"`）と **位置グリッド**（`grid = [...]`）の両方を受け、併用可。
- トリガ名は後述のキー名規則。内容は出力文字列。

## combos（同時押し）

複数キーを同時に押したときの出力。キー順は問わない（正規化されるため `"a,b"` と `"b,a"` は同一エントリ扱い）。出力が空文字 `""` のエントリは無視される（combo として扱わない）。

### 個別形式

```toml
[combos]
"a,b" = "@"        # A と B の同時押し → "@"
"shift,k" = "K"
```

キーは `,` 区切りトリガ名列。

### グリッド形式（トリガごとの配置）

```toml
[combos.k]
grid = [
  [],
  ["ふぁ", "ご", "ふ", "ふぃ", "ふぇ"],
  ["ほ", "じ", "れ", "も", "ゆ"],
  ["づ", "ぞ", "ぼ", "む", "ふぉ"],
]
```

`[combos.<trigger>]` の `<trigger>` がトリガ鍵（複数の場合は `"i,o"` のように引用符付き）。
グリッドの各セルは「トリガ鍵 ＋ その位置の内容鍵」の同時押し → 出力。
DvorakJ の `-XX[...]` ブロックと同形。個別形式とグリッド形式は同一 `[combos]` 内で混在可。

## sequences（順押し）

順序付きキー列を押したときの出力。

```toml
[sequences]
"d,v" = "★"       # D を押してから V
"k,k" = "々"
```

## states（状態 → 層の選択）

修飾や IME の on/off など「どの層をアクティブにするか」だけを決める（状態は入れ子しない）。

```toml
[states]
default = "base"
ime_off = "base"
ime_on  = "kana"
```

> core 側での本格な層切替は未実装。現在は `default` で指定した層が `single_map` に展開される（骨格）。

## キー名規則

トリガ名・`override` の名前キーは以下を使用（dvorakj 互換）:

- 英字 `a`–`z`、数字 `0`–`9`
- 修飾: `shift`/`lshift`/`rshift`, `ctrl`/`lctrl`/`rctrl`, `alt`/`lalt`/`ralt`, `meta`/`lwin` 等
- 特殊: `space`, `enter`, `tab`, `bs`/`backspace`, `esc`/`escape`, `capslock`, 矢印 `left`/`right`/`up`/`down`, `f1`–`f12`
- 記号補完: `-` `=` `` ` `` `\` `[` `]` `;` `'` `,` `.` `/`（`+` は `=` 扱い）

## 出力トークン

セル／出力値は文字列。次の `{...}` 記法で特殊キー出力も表現できる:

- `{enter}`/`{return}` → Enter
- `{bs}`/`{backspace}` → Backspace
- `{left}` `{right}` `{up}` `{down}` → 矢印
- `{tab}` → Tab
- `{esc}`/`{escape}` → Escape
- それ以外はそのままテキスト出力（例: `、{enter}` → 「、」のあと Enter）

## 例

- `config.toml` / `config.json` — 最小構成サンプル（層・combos・sequences・states）
- `shingeta.toml` — 新下駄配列（JIS・同時打鍵）の変換例。`[combos.k]` 等のグリッド形式を使用

## 実装メモ

- パーサ: `sloth-parser`（serde + toml）。`compile_toml` / `compile_json` が `CompiledLayout` を返す。
- 物理テンプレート: us = 4 行 `[13, 13, 11, 10]`、jis = 4 行 `[13, 12, 12, 11]`（数字段は `1`–`0` `-` `^` `¥`）。
- core 側は骨格実装（`SlothLayoutLoader`）: base → `single_map`、combos、`sequences` (len≥2) → `prefix_maps` を結線。named layer / states 切替は TODO。
