# DvorakJ パーサー 設計仕様書

## 1. 概要

Crate `dvorakj-parser` は、DvorakJ 形式の `.txt` レイアウトファイルを `sloth-core::layout::Layout` に変換するパーサーを提供する。`sloth-core`（エンジン本体）と分離し、フォーマット固有の解析責務を独立させることで、将来のフォーマット追加や入れ替えを容易にする。

## 2. 設計目標

- **依存逆転**: パーサーは `sloth-core` のみに依存し、レイアウトファイルフォーマットの詳細をコアから隔離する。
- **キーボード配列対応**: `id` の拡張子でコンパイル対象の物理配列を切り替える（`.en.txt` → US、`.jp.txt` → JIS）。これは入力方式の流派ではなく物理キーボード配列の選択である。
- **文字コード対応**: 入力バイト列を拡張子・BOM・UTF-8妥当性で判定し、UTF-8 または Shift-JIS (`encoding_rs`) としてデコードする。
- **組合せと層**: `同時打鍵` (Combo)、`順次打鍵` (Prefix)、`SandS` (Sustained) の3種類のルーティングを表現する。
- **特殊キー展開**: セル内の `{…}` 構文（`{backspace}` 等）を `cell.rs::brace_token` で `OutputToken` へ展開する。
- **ローマ字変換（未使用経路）**: `KanaEncoder` と `InputMode::Romaji` 経路が実装されているが、現状 `parse_dvorakj` は常に `InputMode::Direct` を渡すため到達しない（§8.3 参照）。

## 3. スコープ

### スコープ内

- `.txt` ファイルのパース（IF02: DvorakJ/新下駄形式）。
- ブロック（`[…]`）、レイヤー、組合せ的なトリガーの抽出。
- セル文字列から `OutputSeq` への変換。
- `{…}` 構文による特殊キー展開。
- コメント除去（`/* … */`）。
- レイアウトモードの自動検出。
- `Shift-JIS` / UTF-8 の自動判別。

### スコープ外

- レイアウトファイルの読み込み、システムフォント描画、ホットキー適用（`sloth-core`・`sloth-daemon` の責務）。
- キャッシュや動的再読み込み。
- 非DvorakJフォーマット（`KKC`、配列ファイルなど、将来は `sloth-config` が追加する）。

## 4. 依存関係

```toml
[dependencies]
sloth-core   = { path = "../sloth-core" }
encoding_rs = "0.8"
```

| クレート | 役割 |
|---------|------|
| `sloth-core` | `Layout`, `LayoutMode`, `KeyCode`, `OutputToken`, `LoadError`, `KeyboardLayout`, `LayoutLoader` を提供。パーサーはこれらを構築する。 |
| `encoding_rs` | Shift-JIS (CP932) → UTF-8 デコード。`.en.txt` は UTF-8、`.jp.txt` は Shift-JIS、その他は UTF-8 判定後に Shift-JIS フォールバック。 |

ディレクトリ `D:\.repo\sloth\dvorakj-parser\` には他に `layout.md`（本ファイル）が存在する。

## 5. アーキテクチャ（モジュール構成）

```
src/
├── lib.rs        # 公開API: DvorakJLayoutLoader
├── parse.rs      # 行パーサ・レイヤールート決定
├── block.rs      # `[…]` ブロック抽出、レイヤー名正規化、tap行分割
├── grid.rs       # `|` delim行 → 物理キー (KeyCode) へのマッピング
├── cell.rs       # セル内のローマ字・特殊キー・braceトークン展開
└── keymap.rs     # 新下駄スキャンコード (0x02〜0x7D) → KeyCode 変換表
```

責務分担は以下の通り：

| ファイル | 責務 |
|---------|------|
| `lib.rs` | LayoutLoader 実装、ファイル内容の文字デコード、コメント除去、パーサ呼び出し |
| `parse.rs` | `Layout` の構築（モード検出、レイヤー名—トリガー対応付け、ルーティング） |
| `block.rs` | テキスト上での矩形ブロック抽出、レイヤー名の引数正規化、行の tap/separation |
| `grid.rs` | 物理配列ストリップ（US/JIS 別）に基づくグリッドパース |
| `cell.rs` | セル文字列 → `OutputSeq` compile、`{…}` brace特殊キー、ローマ字ascii mapping |
| `keymap.rs` | 新下駄スキャンコード用ルックアップテーブル (u32 → KeyCode) |

## 6. 公開API

### 6.1 `DvorakJLayoutLoader`

```rust
pub struct DvorakJLayoutLoader { /* fields internal */ }
impl Default for DvorakJLayoutLoader { ... }
impl LayoutLoader for DvorakJLayoutLoader { ... }
```

`LayoutLoader` トレイトを実装する唯一の型。`sloth-core` からはトレイト経由で使われる。`new()` と `Default` の両方で構築可能（内部に `KanaEncoder` を保持）。

- `format_name() -> &'static str` 常に `"dvorakj"` を返す。
- `load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError>`
  - `id` の拡張子 `".en.txt"` / `".jp.txt"`、または `UTF-8`/Shift-JIS 自動判定によりデコード。
  - `strip_comments` で `/* … */` コメントブロックを除去。
  - 内部関数 `parse_dvorakj` に委譲し、`Layout` を構築する。

### 6.2 エラー処理

`load` / `parse_dvorakj` のシグネチャは `Result<Layout, LoadError>` だが、**現実装は実質的にエラーを返さない**（常に `Ok` を返す寛容パーサ）。

- `resolve_trigger` は `LoadError::UnknownTrigger` を構築するが、唯一の呼び出し元 `resolve_trigger_spec` が `if let Ok(kc) = resolve_trigger(...)`（parse.rs:105）で結果を握り潰し、未解決トリガーは `Ok(None)` として静かにスキップする。
- 未定義レイヤー名・未知スキャンコードを含むブロックは、エラーではなく**そのブロックごとスキップ**される（parse.rs:213, 306）。
- `compile_cell` / `parse_grid` は常に `Ok` を返し、`LoadError::Parse` を生成する箇所は存在しない。
- したがって `?` は構文上存在するが、通常の入力で発火することはない。

`LoadError` の全バリアント（`Io` / `Encoding` / `Parse` / `UnknownTrigger` / `Schema`）は `sloth-core::loader` に定義されている。

## 7. 入力形式の概要（DvorakJ/新下駄 `.txt`）

```
<任意の名前 / レイアウト名>
/* C-style コメントは除去 */

/* 項目 #1: トリガー定義（複数可）。ブロックは `[` ... `]` */
-option-input
[
{layer-name} | -10
[k]          | [shift]
q            | ([q]
]

/* 項目 #2: base grid */
[
key1|key2|...
]

/* `{...}` レイヤーブロック */
{layer-name}
[
key1|key2|...
]

/* `-...` レイヤーブロック。`-d-k` は `d`, `k` の複数レイヤー名として扱う */
-d-k
[
key1|key2|...
]

/* bracket-named レイヤーブロック: ヘッダーの `[d],[k]` がレイヤー名、最後の `[` 以降が grid */
[d],[k][
key1|key2|...
]

/* paren-wrapped ブロック: Mixed mode で同時打鍵候補として扱う */
({layer-name}
[
key1|key2|...
]

/* tap 行: 最終行が 1〜2セルかつ非空なら tap-out として grid から分離 */
[
key1|key2|...
tapCell
]
```

## 8. 主要データ構造と流れ

### 8.1 パースの主要フェーズ

1. **文字デコード** (`lib.rs:load`)
   - `id.ends_with(".en.txt")` → UTF-8 即時。
   - `id.ends_with(".jp.txt")` → Shift-JIS デコード。
   - その他 → UTF-8 BOM 取り除き、UTF-8 失敗なら SHIFT_JIS にフォールバック。

2. **コメント除去** (`lib.rs:strip_comments`)
   - 単一パス、`/*` `*/` のネストは未対応（DvorakJ ファイルは通常単層のため）。

3. **Layout 初期化**
   - `mode`: 行頭名に `順` → Sequential、`同時` → Simultaneous、両方 → Mixed、無し → Legacy。
   - `layer_triggers`: 初期値として `"shift"` → `KeyCode::ShiftL`。
   - `sustained_names`: `"shift"` を初期化。

4. **行ごと分岐**（実コードでの判定順）
   1. `-option-input` ブロック: トリガー抽出、`sustained_names` / `combo_capable_names` 更新。
   2. `[` 始まり かつ bracket-named（最後の `[` より前に `]` がある、例 `[d],[k][`）: bracket-named レイヤーブロック。
   3. `[` 始まり（上記以外）: base grid → `single_map` 更新、`base_row_count` 更新。
   4. `(…` を剥がした先頭が `{` または `-`（`-option-input` を除く）: `{…}` / `-…` レイヤーブロック。`(` 付きは Mixed mode で同時打鍵候補（`is_paren`）。

5. **レイヤーブロック構築**
   - レイヤー名 → `KeyCode`（`layer_triggers` の既存 or スキャンコードから解決）。
   - `split_tap_row` で最終 tap-out 行を分離。
   - `parse_grid` でペア (`KeyCode`, `OutputSeq`) を生成。
   - `determine_route` で `BlockRoute` を決定し `apply_route` で `Layout` に注入。
   - trigger 自身の layer taps も反映。

### 8.2 ルーティング決定

`determine_route(mode, is_sustained, is_paren, is_combo_capable) -> BlockRoute`

| `mode` | `is_sustained` | `is_paren` | `is_combo_capable` | BlockRoute |
|--------|---------------|-----------|-------------------|------------|
| any | true | - | - | `Sustained` |
| Legacy/Simultaneous | false | - | - | `Combo` |
| Sequential | false | - | - | `Prefix` |
| Mixed | false | true | - | `Combo` |
| Mixed | false | false | true | `PrefixAndCombo` |
| Mixed | false | false | false | `Prefix` |

`is_sustained` が真なら（determine_route の早期 return により）全モードで `Sustained` が優先される。

`BlockRoute` が各レイヤーとグリッドを `Layout.layer_maps`, `Layout.combos`, `Layout.prefix_maps`, `Layout.layer_triggers` 等に配分する。

### 8.3 セルコンパイル (`cell.rs`)

- 空 or `@@@`: 出力なし。
- `Romaji` InputMode 且つ `{…}` なし: `KanaEncoder::encode` によりローマ字1文字を1キーに変換。**ただし `parse_dvorakj` は常に `InputMode::Direct` を渡すため、この経路は現状デッドコードである**（`KanaEncoder` / `encode` / Romaji 分岐は将来のローマ字入力対応用に予約）。
- それ以外（= 常に通る `Direct` 経路）: char 単位に `key_or_text` を適用。ASCII 英数字は `KeyCode`、それ以外は US 配列なら `OutputToken::Text`、未知文字も `Text`。大文字は `Modifiers::SHIFT` 付与。
- `{…}` 内は `brace_token` で特殊キー名にマッチ（`{backspace}`/`{bs}` = `SpecialKey::Backspace`、`{enter}`、`{space}`、`{pipe}`/`{bar}` = `"|"` 等）。閉じ `}` が無い場合は `{` 以降を素の文字列として展開。

## 9. 出力: `Layout` 構造

`sloth-core` (`D:\.repo\sloth\sloth-core\src\layout.rs`) の定義を参照。主要フィールドは次の通り：

| フィールド | 説明 |
|-----------|------|
| `id` | ファイル名 |
| `name` | 最初の非空行（レイアウト名）|
| `mode` | モード (Legacy/Sequential/Simultaneous/Mixed) |
| `input_mode` | 常に `InputMode::Direct`（ローマ字変換はレイアウト内部のセルで処理）|
| `keyboard` | `KeyboardLayout::Us` or `Jis` |
| `single_map` | base grid: `KeyCode → OutputSeq` |
| `layer_maps` | 保持レイヤー (sorted active keys): `Vec<KeyCode> → HashMap<KeyCode, OutputSeq>` |
| `layer_taps` | tap solo output: `KeyCode → OutputSeq` |
| `layer_triggers` | 全トリガーキー集合 |
| `combos` | 同時打鍵（k, key...）: sorted chord set → OutputSeq |
| `combo_keys` | 組合せに参加するキー集合 |
| `sustained_triggers` | 保持/SandS 風トリガー |
| `prefix_maps` | 順次 trigger: `[key,...] → inner map` |
| `prefix_triggers` | 順次 trigger 集合 |

## 10. 文字コード/キー配列対応

### 10.1 キー配列 (`grid.rs:physical_row`)

物理キー配列パターンは 4行 x US/JIS の 2つを静的に持つ：

- JIS row: (NumRow) QWERTY row → ASDF row → ZXCV row
- US row: (NumRow) QWERTY row → ASDF row → ZXCV row

配列変更は `sloth-core` の `physical_keycode_for` 相当の依存への移行を推奨するが、現段階ではハードコードされた静的配列を用いる。

## 11. セキュリティ・エラー境界

- `encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()` は不正バイトを U+FFFD 置換文字に変換する（`had_errors` フラグは参照せず、デコード失敗でエラーにはならない）。
- BOM は明示的にストリップする (`strip_prefix([0xEF,0xBB,0xBF])`)。
- 未定義レイヤー名・未知トリガーはエラーにせず該当ブロックをスキップする寛容パーサ方針のため、入力ミスがあっても部分的な `Layout` が生成される（§6.2 参照）。検証を強化する場合はこの方針の見直しが必要。

## 12. 今後の展望（スコープ拡大へ向けた注意点）

- 静的キー配列 (`physical_row`) は、US/JIS 以外のレイアウト（Dvorak、Colemak 等）を扱う場合は要外部化。
- `-option-input` のネストやブロック重複は DvorakJ の仕様上現実的には発生しないため検証コーナーケースに留める。
- `KanaEncoder` のローマ字テーブル拡張は `sloth-config` 等の改良で対応する。
- テスト値の I/O 例ファイル (`tests/fixtures/*.txt`) は設計検証に有用。
