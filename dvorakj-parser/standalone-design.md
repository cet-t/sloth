# dvorakj-parser 独立公開化 設計書

## 1. 目的

`dvorakj-parser/src/` を `sloth-core` に依存しない独立パーサーとして切り出し、外部 crate として公開可能な構成にする。

現状の `dvorakj-parser` は DvorakJ 形式 `.txt` を解析できるが、出力型・エラー型・キー型・レイアウト型をすべて `sloth-core` に依存している。そのため、外部利用者は `sloth-core` も同時に取り込む必要がある。

独立化後は、パーサー crate 自身が DvorakJ フォーマットのドメイン型を持ち、`sloth` 側は変換アダプタで `sloth-core::Layout` に変換する。

## 2. 方針

### 2.1 rlib は依存ゼロ、cdylib は単体利用優先

目標は 2 つある。

1. Rust crate (`rlib`) としては `sloth-core` に依存しない中立 parser にする。
2. `dvorakj-parser.dll` / `libdvorakj_parser.so` としては単体配布でき、呼び出し側が `sloth-core` や Shift-JIS decoder を持たなくても使えるようにする。

そのため、default feature は依存ゼロを維持し、DLL/SO 配布用 feature では FFI と Shift-JIS/JSON 出力に必要な依存を有効化する。

```toml
[lib]
crate-type = ["rlib", "cdylib"]

[features]
default = []
encoding = ["dep:encoding_rs"]
json = ["dep:serde", "dep:serde_json"]
ffi = ["json"]
cdylib-full = ["ffi", "encoding"]

# 注意: `ffi` は `encoding` を含めない。バイト列入力の `dvorakj_parse_json` は
# `encoding` を要するため `#[cfg(all(feature = "ffi", feature = "encoding"))]`
# でゲートし、`encoding` 無し（`--features ffi` 単体）では
# `dvorakj_parse_str_json`（UTF-8 文字列入力）のみを公開する（§18.4 参照）。

[dependencies]
encoding_rs = { version = "0.8", optional = true }
serde = { version = "1", features = ["derive"], optional = true }
serde_json = { version = "1", optional = true }
```

| 利用形態 | feature | 入力 | 出力 | 外部依存 |
|----------|---------|------|------|----------|
| Rust rlib 最小 | `default-features = false` | `&str` | `ParsedLayout` | なし |
| Rust rlib byte対応 | `encoding` | `&[u8]` | `ParsedLayout` | `encoding_rs` |
| DLL/SO 単体 | `cdylib-full` | `*const u8 + len` | JSON文字列 | `encoding_rs`, `serde`, `serde_json` をDLL内に同梱 |

理由:

- Rust 利用者には依存ゼロの軽量 parser を提供する。
- DLL/SO 利用者には「ファイルバイト列を渡すだけでJSONが返る」単体利用体験を提供する。
- `sloth-core` への変換は sloth 側 adapter に閉じ込める。

### 2.2 Shift-JIS デコード方針

Rust 標準ライブラリだけでは Shift-JIS/CP932 デコードはできない。したがって、default API は **デコード済み `&str` を受け取る**。

```rust
pub fn parse_str(text: &str, options: ParseOptions) -> ParseResult<ParseReport>;
```

一方、DLL/SO 単体利用では `.jp.txt` の Shift-JIS バイト列をそのまま処理できる必要がある。そのため `encoding` feature 有効時に `parse_bytes` を提供し、`cdylib-full` では `encoding` を必須にする。

```rust
#[cfg(feature = "encoding")]
pub fn parse_bytes(bytes: &[u8], source_id: &str, options: ParseOptions) -> ParseResult<ParseReport>;
```

デコード規則は現状互換:

| 条件 | keyboard default | decode |
|------|------------------|--------|
| `source_id.ends_with(".en.txt")` | `KeyboardLayout::Us` | UTF-8 lossy |
| `source_id.ends_with(".jp.txt")` | `KeyboardLayout::Jis` | Shift-JIS |
| その他 | `KeyboardLayout::Jis` | BOM strip → UTF-8 → Shift-JIS fallback |

自前 Shift-JIS decoder 実装は保守コストが高いため不採用。依存ゼロは rlib default のみに限定し、DLL/SO 単体利用では `encoding_rs` を内蔵する。

## 3. 現状との差分

| 項目 | 現状 | 独立化後 |
|------|------|----------|
| 公開型 | `DvorakJLayoutLoader` が `sloth_core::loader::LayoutLoader` を実装 | `DvorakJParser` / free function `parse_str` を公開 |
| 入力 | `load(bytes, id)` | `parse_str(text, options)`。byte decode は別層 |
| 出力 | `sloth_core::layout::Layout` | `dvorakj_parser::ParsedLayout` |
| キー型 | `sloth_core::KeyCode` | `dvorakj_parser::Key` |
| 出力トークン | `sloth_core::OutputToken` | `dvorakj_parser::OutputToken` |
| 修飾キー | `sloth_core::Modifiers` (`bitflags`) | `dvorakj_parser::Modifiers(u16)` |
| キーボード配列 | `sloth_core::KeyboardLayout` | `dvorakj_parser::KeyboardLayout` |
| レイアウトモード | `sloth_core::layout::LayoutMode` | `dvorakj_parser::LayoutMode` |
| エラー型 | `sloth_core::loader::LoadError` | `dvorakj_parser::ParseError` |
| canonical sort | `sloth_core::layout::canon_sort` | parser 内 `sort_keys_canonical` |
| DvorakJ key name | `KeyCode::from_dvorakj_name` | parser 内 `Key::from_dvorakj_name` |
| Shift-JIS | `encoding_rs` 直接依存 | default なし。`encoding` feature または sloth adapter |
| sloth 統合 | `sloth-daemon` が `DvorakJLayoutLoader` を登録 | `sloth` 側 adapter が `ParsedLayout -> sloth_core::Layout` 変換 |
| ライブラリ種別 | `rlib` のみ（暗黙） | `crate-type = ["rlib", "cdylib"]` |
| DLL/SO FFI | なし | `extern "C"` 入口（`dvorakj_parse_json` 他）を `ffi` feature で提供 |
| DLL 出力形式 | なし | JSON 文字列（`serde_json`）+ 解放関数 |

## 4. 依存箇所の棚卸し

現状 `src/` の `sloth_core` 依存は以下。

| ファイル | 依存型/関数 | 独立化方針 |
|----------|-------------|------------|
| `lib.rs` | `Layout`, `LayoutLoader`, `LoadError`, `KeyboardLayout` | loader 実装を sloth adapter へ移動。parser crate は `parse_str` を公開 |
| `parse.rs` | `Layout`, `LayoutMode`, `LoadError`, `InputMode`, `KeyCode`, `KeyboardLayout`, `Modifiers`, `OutputToken`, `canon_sort` | すべて `model.rs` の自前型へ置換 |
| `block.rs` | `KeyCode` | `Key` に置換 |
| `cell.rs` | `LoadError`, `InputMode`, `KeyCode`, `KeyboardLayout`, `Modifiers`, `OutputSeq`, `OutputToken`, `SpecialKey` | `model.rs` の自前型へ置換 |
| `grid.rs` | `LoadError`, `InputMode`, `KeyCode`, `KeyboardLayout`, `OutputSeq` | `model.rs` の自前型へ置換 |
| `keymap.rs` | `KeyCode` | `Key` に置換 |

## 5. 新しい crate 構成

```text
dvorakj-parser/
├── Cargo.toml
├── cbindgen.toml       # C ヘッダ生成設定 (ffi feature)
├── README.md
├── include/
│   └── dvorakj_parser.h # cbindgen 生成 or 手書き C ヘッダ
├── src/
│   ├── lib.rs          # public API export
│   ├── model.rs        # 依存ゼロの公開ドメイン型
│   ├── parse.rs        # DvorakJ parser 本体
│   ├── block.rs        # block helper
│   ├── grid.rs         # physical grid parser
│   ├── cell.rs         # cell compiler
│   ├── keymap.rs       # scan-code -> Key
│   ├── decode.rs       # optional `encoding` feature 用
│   ├── serde_impl.rs   # optional `json` feature 用 Serialize 実装
│   └── ffi.rs          # optional `ffi` feature 用 extern "C" 層
├── examples/
│   └── c_consumer/     # DLL/SO を dlopen/LoadLibrary する C 最小例
└── tests/
    ├── fixtures/
    ├── compatibility.rs
    └── ffi_roundtrip.rs
```

`lib.rs` は `sloth-core` を一切参照しない。

```rust
mod block;
mod cell;
mod grid;
mod keymap;
mod model;
mod parse;

pub use model::*;
pub use parse::{parse_str, DvorakJParser};

#[cfg(feature = "encoding")]
pub mod decode;

#[cfg(feature = "json")]
mod serde_impl;

#[cfg(feature = "ffi")]
pub mod ffi;
```

## 6. 公開API設計

### 6.1 最小API

```rust
pub fn parse_str(text: &str, options: ParseOptions) -> ParseResult<ParseReport>;
```

### 6.2 Parser struct API

設定を保持したい利用者向けに `DvorakJParser` も提供する。

```rust
#[derive(Debug, Clone)]
pub struct DvorakJParser {
    options: ParseOptions,
}

impl DvorakJParser {
    pub fn new(options: ParseOptions) -> Self;
    pub fn parse_str(&self, text: &str) -> ParseResult<ParseReport>;
}
```

### 6.3 Optional byte API

`encoding` feature 有効時のみ `parse_bytes` を提供する。シグネチャとデコード規則は §2.2 を参照。依存ゼロ default (rlib) API ではバイト列デコードをしない。

### 6.4 C ABI 入口

`ffi` feature 有効時、DLL/SO 単体利用のための `extern "C"` 関数群を提供する。詳細は §18 を参照。Rust 利用者は §6.1〜6.3 の型付き API を、非 Rust 利用者は §18 の C ABI を使う。

## 7. 独立ドメイン型

### 7.1 `ParsedLayout`

`sloth_core::Layout` とほぼ同じ構造を維持するが、名称を parser crate の文脈に合わせる。

```rust
#[derive(Debug, Clone, Default)]
pub struct ParsedLayout {
    pub source_id: Option<String>,
    pub name: String,
    pub mode: LayoutMode,
    pub input_mode: InputMode,
    pub keyboard: KeyboardLayout,
    pub single_map: BTreeMap<Key, OutputSeq>,
    pub layer_maps: BTreeMap<KeyChord, BTreeMap<Key, OutputSeq>>,
    pub layer_taps: BTreeMap<Key, OutputSeq>,
    pub layer_triggers: BTreeSet<Key>,
    pub combos: BTreeMap<KeyChord, OutputSeq>,
    pub combo_keys: BTreeSet<Key>,
    pub sustained_triggers: BTreeSet<Key>,
    pub prefix_maps: BTreeMap<KeyChord, BTreeMap<Key, OutputSeq>>,
    pub prefix_triggers: BTreeSet<Key>,
}
```

現状 `sloth_core::Layout` にある `id: LayoutId` と `simultaneous: Vec<ComboRule>` は、`ParsedLayout` では持たない。

- `id`: `ParsedLayout` では `source_id: Option<String>` に改名する。ただし現状 `parse_dvorakj` は `id` を必ず設定するのに対し、独立化後は「parser は source_id を任意メタデータとして持つだけ」とし、`sloth_core::Layout.id` の確定は adapter の `convert_layout(report.layout, id)` 側で行う。したがって source_id は情報提供目的の任意フィールドであり、adapter が渡す `id` が最終的な真実である（二重管理にしない）。
- `simultaneous`: 現状 `parse_dvorakj` は常に `vec![]` を設定しており（`ComboRule` は生成されない）、実質デッドフィールドである。よって `ParsedLayout` からは**意図的に廃止**する。`ComboRule` 型も parser crate へは移植しない。§13.1 の互換チェック対象にも含めない。

現状は `HashMap` / `HashSet` を使うが、公開 API では再現性のため `BTreeMap` / `BTreeSet` を推奨する。外部公開 crate では JSON snapshots やテスト比較が安定するため。JSON 出力（`json` feature）でもキー順が決まり差分が安定する。

互換性最優先なら内部パースは `HashMap` のまま、公開前に canonical order で `BTree*` へ変換してもよい。

`json` feature 有効時は、全公開型に `#[cfg_attr(feature = "json", derive(serde::Serialize))]` を付与する（または `serde_impl.rs` に手実装）。default (依存ゼロ) では serde を導出しない。`BTreeMap<KeyChord, _>` を JSON にする際、`KeyChord` は map キーになれないため、JSON 表現では chord を配列オブジェクト（`{"keys": [...], "output": ...}` のリスト）として直列化する専用表現を用いる（§18.5 参照）。

### 7.2 `Key`

`sloth_core::KeyCode` に依存せず、DvorakJ parser に必要なキー集合を保持する。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    Minus, Equal, LBracket, RBracket, Backslash, Semicolon, Quote, Comma, Dot, Slash, Grave,
    ShiftL, ShiftR, CtrlL, CtrlR, AltL, AltR, MetaL, MetaR,
    Space, Enter, Tab, Backspace, Escape, CapsLock,
    Left, Right, Up, Down,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Muhenkan, Henkan, KanaKatakana, HankakuZenkaku,
    Yen, Caret, Colon, AtSign,
    Unknown(u32),
}
```

`Key::from_dvorakj_name(name: &str) -> Option<Key>` を parser crate に移動する。

### 7.3 `Modifiers`

`bitflags` 依存を避けるため、自前 newtype で表現する。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u16);

impl Modifiers {
    pub const SHIFT: Self = Self(0b0000_0000_0000_0001);
    pub const CTRL: Self = Self(0b0000_0000_0000_0010);
    pub const ALT: Self = Self(0b0000_0000_0000_0100);
    pub const META: Self = Self(0b0000_0000_0000_1000);

    pub const fn empty() -> Self;
    pub const fn bits(self) -> u16;
    pub const fn contains(self, other: Self) -> bool;
    pub fn insert(&mut self, other: Self);
}
```

現状 parser が使っているのは `empty()` と `SHIFT` のみ。最初は最小実装で足りる。

### 7.4 `OutputToken`

```rust
pub type OutputSeq = Vec<OutputToken>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputToken {
    Key { code: Key, mods: Modifiers },
    Text(String),
    Named(SpecialKey),
    ModDown(Key),
    ModUp(Key),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialKey {
    Backspace,
    Enter,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
}
```

### 7.5 `KeyChord`

現状は `Vec<KeyCode>` を canonical sort してキーとしている。公開 API では専用型にする。

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyChord(Vec<Key>);

impl KeyChord {
    pub fn new(keys: impl Into<Vec<Key>>) -> Self;
    pub fn as_slice(&self) -> &[Key];
    pub fn into_vec(self) -> Vec<Key>;
}
```

`KeyChord::new` は必ず `sort_keys_canonical` を適用する。これにより chord key の順序揺れを外部 API から隠蔽する。

## 8. エラー・警告方針

現状 parser は未知トリガーや未定義レイヤーを静かにスキップする。公開 parser としては、互換性と検証性の両方が必要。

### 8.1 推奨API

```rust
pub type ParseResult<T> = Result<T, ParseError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseReport {
    pub layout: ParsedLayout,
    pub warnings: Vec<ParseWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    // decode 系（`encoding` feature の parse_bytes / FFI バイト入力でのみ発生）。
    // `parse_str(&str)` は UTF-8 済み入力を受けるため、これらは返さない。
    UnsupportedEncoding,
    InvalidUtf8,
    // parse 系（strict モードで発生）。
    UnknownTrigger { value: String, line: Option<usize> },
    MalformedBlock { line: Option<usize>, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseWarning {
    UnknownTrigger { value: String, line: Option<usize> },
    MissingLayer { name: String, line: Option<usize> },
    SkippedBlock { line: Option<usize>, reason: String },
    DecodeReplacement { source_id: Option<String> },
}
```

### 8.2 互換モードと strict モード

```rust
#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub source_id: Option<String>,
    pub keyboard: KeyboardLayout,
    pub strict: bool,
}

impl ParseOptions {
    pub fn from_source_id(source_id: impl Into<String>) -> Self;
}

impl KeyboardLayout {
    pub fn from_source_id(source_id: &str) -> Self; // .en.txt = Us, otherwise Jis
}
```

| `strict` | 挙動 |
|----------|------|
| `false` | 現状互換。未知トリガーや未定義レイヤーはスキップし、`warnings` に記録 |
| `true` | 未知トリガーや不正ブロックを `ParseError` として返す |

外部公開では `strict=false` を default にし、既存 DvorakJ コーパスを壊さない。CLI やテスト用途では `strict=true` を使えるようにする。

## 9. canonical sort の独立化

現状 `parse.rs` は `sloth_core::layout::canon_sort` を呼ぶ。独立化後は parser crate 内に同等実装を持つ。

```rust
pub fn canonical_key_order(k: Key) -> u16 {
    match k {
        Key::Space => 1,
        Key::ShiftL => 2,
        Key::ShiftR => 3,
        Key::CtrlL => 4,
        Key::CtrlR => 5,
        Key::AltL => 6,
        Key::AltR => 7,
        Key::MetaL => 8,
        Key::MetaR => 9,
        Key::Muhenkan => 10,
        Key::Henkan => 11,
        Key::KanaKatakana => 12,
        Key::HankakuZenkaku => 13,
        Key::Yen => 14,
        Key::Caret => 15,
        Key::Colon => 16,
        Key::AtSign => 17,
        Key::Unknown(_) => 200,
        _ => 100,
    }
}

pub fn sort_keys_canonical(keys: &mut [Key]) {
    keys.sort_by_key(|k| (canonical_key_order(*k), format!("{:?}", k)));
}
```

#### 9.1 統合時の互換性注意（重要）

現状コードには **2 種類の順序** が併存している。

| 用途 | 実装 | tie-break |
|------|------|-----------|
| chord（combos のキー） | `sloth_core::layout::canon_sort` (`parse.rs` の `canon_sort(&mut chord)`) | rank + `format!("{:?}", k)` |
| layer キー列（`layer_maps` / `prefix_maps` のキー `Vec<KeyCode>`） | `layer_ks.sort_by_key(\|k\| key_sort(*k))` (`parse.rs:217, 310`) | **rank のみ**（同 rank 内は stable sort により入力順を維持） |

`block.rs::key_sort` は rank のみを返す関数で、Debug tie-break を持たない。したがって `key_sort` を tie-break 付きの `sort_keys_canonical` へ単純統合すると、**同 rank 内（例: 英字はすべて rank 100）の layer キー列の並びが変わり得る**。

具体例: 複数レイヤー名ブロック `-d-k` や bracket-named `[k],[d]` は、現状 `key_sort` では入力順（`[d, k]` や `[k, d]`）がそのまま `layer_maps` / `prefix_maps` のキーになる。`sort_keys_canonical` を適用すると必ず Debug 名順（`[d, k]`）に正規化されるため、入力順が `[k, d]` だったブロックはキーが変化する。

これは §13.1 の互換要件「layer maps 一致」を破り得るため、統合時は次のいずれかを選ぶ。

- **方針A（推奨・互換優先）**: layer キー列も canonical order に統一する。実行時マッチャ側も同じ canonical order で押下キーを整列するなら意味論は不変。ただし現状マッチャが `key_sort` 順に依存していないことを確認し、fixture で `layer_maps` / `prefix_maps` のキー集合一致を検証する。
- **方針B（現状順を厳密保持）**: `KeyChord` は combos 専用とし、`layer_maps` / `prefix_maps` のキーは現状どおり「rank のみ + 入力順」を維持する別経路（`sort_by_rank_stable` 等）を残す。

単要素の `prefix_maps`（`apply_route` は常に `vec![k]` を挿入）は要素数 1 のため、この順序問題の影響を受けない。影響があるのは 2 個以上のレイヤー名を持つ `layer_maps` エントリのみ。

いずれの方針でも、独立化前後で `sloth_core::Layout` に戻した結果の `layer_maps` / `prefix_maps` キー集合が一致することを fixture test（§13.1）で必ず確認する。

`block.rs::key_sort` はこの方針に沿って `canonical_key_order` へ統合する（方針B を採る場合は tie-break を分離する）。

## 10. sloth との統合設計

独立 parser は `sloth-core::LayoutLoader` を実装しない。`sloth` 側で adapter を持つ。

### 10.1 Adapter の配置

候補:

| 配置 | メリット | デメリット |
|------|----------|------------|
| `sloth-daemon` 内 | 依存が局所化する | 他バイナリで再利用しづらい |
| `sloth-config` 内 | 設定/レイアウト読込に近い | daemon 以外でも config crate が必要 |
| 新 crate `sloth-dvorakj-adapter` | 責務が明確 | crate が増える |

推奨は短期では `sloth-daemon` 内、長期では `sloth-dvorakj-adapter`。

### 10.2 Adapter API

```rust
#[derive(Default)]
struct RmapDvorakJLayoutLoader;

impl RmapDvorakJLayoutLoader {
    fn new() -> Self {
        Self
    }
}

impl sloth_core::loader::LayoutLoader for RmapDvorakJLayoutLoader {
    fn format_name(&self) -> &'static str { "dvorakj" }

    fn load(&self, bytes: &[u8], id: &str) -> Result<sloth_core::layout::Layout, sloth_core::loader::LoadError> {
        let text = decode_like_current_loader(bytes, id)?;
        let options = dvorakj_parser::ParseOptions::from_source_id(id);
        let report = dvorakj_parser::parse_str(&text, options)
            .map_err(convert_parse_error)?;
        Ok(convert_layout(report.layout, id))
    }
}
```

`sloth-daemon/src/main.rs` は次のように変わる。

現状:

```rust
sloth_core::loader::register_default_loader(
    Box::new(dvorakj_parser::DvorakJLayoutLoader::new()),
);
```

独立化後:

```rust
sloth_core::loader::register_default_loader(
    Box::new(RmapDvorakJLayoutLoader::new()),
);
```

## 11. 変換アダプタ設計

`dvorakj_parser::Key` と `sloth_core::KeyCode` は別型にする。変換は sloth 側に閉じ込める。

```rust
fn to_core_key(k: dvorakj_parser::Key) -> sloth_core::KeyCode;
fn to_core_mods(m: dvorakj_parser::Modifiers) -> sloth_core::Modifiers;
fn to_core_output(t: dvorakj_parser::OutputToken) -> sloth_core::OutputToken;
fn to_core_layout(l: dvorakj_parser::ParsedLayout, id: &str) -> sloth_core::layout::Layout;
```

変換は全 variant を網羅する `match` にする。`Unknown(u32)` も維持する。

変換ミスを防ぐため、`Key` と `KeyCode` の variant 名は当面一致させる。

## 12. ファイル別修正方針

### 12.1 `src/lib.rs`

現状:

- `DvorakJLayoutLoader` を公開。
- `LayoutLoader` trait 実装。
- byte decode と comment strip と parse 呼び出しを同居。

修正後:

- `DvorakJLayoutLoader` を削除、または `sloth` adapter 側へ移動。
- `parse_str`, `DvorakJParser`, model types を export。
- `strip_comments` は parser crate 内 utility として維持。
- byte decode は `decode.rs` optional feature へ移動。

### 12.2 `src/model.rs`

新設。以下を定義する。

- `Key`
- `KeyboardLayout`
- `LayoutMode`
- `InputMode`
- `Modifiers`
- `OutputToken`
- `SpecialKey`
- `OutputSeq`
- `ParsedLayout`
- `KeyChord`
- `ParseOptions`
- `ParseReport`
- `ParseError`
- `ParseWarning`

`KanaEncoder` の扱い（決定事項）:

現状 `parse_dvorakj` と `compile_cell` は `encoder: &KanaEncoder` を引数に取るが、`parse_dvorakj` は常に `InputMode::Direct` を渡すため `KanaEncoder::encode` / `InputMode::Romaji` 経路は**デッドコード**（layout.md §8.3 参照）。独立化にあたり:

- 新 API `parse_str(text, options)` は `encoder` 引数を持たない。
- `KanaEncoder` は `model.rs` へは移植せず、**当面削除する**（`compile_cell` からも `encoder` 引数を除去）。
- `InputMode` 型自体は将来のローマ字/かな入力対応の予約として残すが、parser は常に `Direct` を出力する。将来ローマ字対応を復活させる場合は、別途 encoder を feature 付きで再導入する設計を起こす。

### 12.3 `src/parse.rs`

現状:

- `sloth_core::Layout` を直接構築。
- `LoadError` を使うが実質的には返さない。
- `canon_sort` を `sloth_core` から呼ぶ。

修正後:

- `ParsedLayout` を構築。
- `ParseReport { layout, warnings }` を返す。
- strict モードなら `ParseError` を返す。
- `KeyChord::new` / `sort_keys_canonical` を使う。

### 12.4 `src/block.rs`

現状:

- `key_sort(k: KeyCode)` のためだけに `sloth_core::KeyCode` へ依存。

修正後:

- `key_sort` を削除し `canonical_key_order` へ統合。
- もしくは `Key` を受け取る関数へ変更。

### 12.5 `src/cell.rs`

現状:

- `LoadError`, `InputMode`, `KeyCode`, `KeyboardLayout`, `Modifiers`, `OutputToken`, `SpecialKey` へ依存。
- `compile_cell` は常に `Ok` だが `Result` を返す。
- `compile_cell` は `encoder: &KanaEncoder` 引数を取るが、`InputMode::Direct` 固定のため未使用。

修正後:

- 自前型へ置換。
- `KanaEncoder` 引数を削除する（§12.2 の決定に従いデッドコードを除去）。
- `compile_cell` はエラーを返さないなら `OutputSeq` を直接返す。
- strict でセル構文エラーを扱いたい場合のみ `Result` を残す。

推奨:

```rust
pub(crate) fn compile_cell(cell: &str, keyboard: KeyboardLayout) -> OutputSeq
```

閉じない `{` は現状どおり素文字列展開なのでエラーにしない。

### 12.6 `src/grid.rs`

現状:

- `parse_grid` は常に `Ok` だが `Result<HashMap<...>, LoadError>` を返す。

修正後:

- `BTreeMap<Key, OutputSeq>` を返す。
- `compile_cell` が非 `Result` なら `parse_grid` も非 `Result` にする。
- `compile_cell` から `encoder` / `InputMode` 引数が消えるのに合わせ、`parse_grid` からも `encoder`・`InputMode` 引数を削除する（残すのは `grid_body`, `offset`, `keyboard`）。

### 12.7 `src/keymap.rs`

現状:

- `keycode_from_scancode(code) -> Option<KeyCode>`。

修正後:

- `key_from_scancode(code) -> Option<Key>`。

## 13. 互換性維持

### 13.1 パース結果互換

独立化前後で、同じ DvorakJ 入力に対して以下が一致する必要がある。

- layout name
- mode
- keyboard layout
- base grid mapping
- layer maps
- layer taps
- combos
- combo keys
- sustained triggers
- prefix maps
- prefix triggers

`sloth` adapter 経由で `sloth_core::Layout` に変換した結果が、現在の `DvorakJLayoutLoader` 出力と一致することを fixture test で確認する。

### 13.2 警告の追加は互換扱い

現状は未知 trigger を黙ってスキップする。独立化後は `warnings` に記録するが、`strict=false` では `layout` の中身は現状と一致させる。

### 13.3 HashMap から BTreeMap への差分

内部表現が `BTreeMap` になる場合、反復順は変わるが意味論は同じ。`sloth` 変換時には `HashMap` に戻してよい。

## 14. 実装ステップ

1. `model.rs` を追加し、`sloth-core` 由来の最小型を parser crate に移植する。
2. `keymap.rs`, `cell.rs`, `grid.rs`, `block.rs` を `model.rs` 型へ置換する。
3. `parse.rs` の出力を `ParsedLayout` / `ParseReport` に変更する。
4. `lib.rs` から `LayoutLoader` 実装を除去し、`parse_str` API を公開する。
5. optional `decode.rs`（`encoding` feature）で `parse_bytes` を追加する。
6. `Cargo.toml` に `crate-type = ["rlib", "cdylib"]` と feature 群を追加する。
7. `json` feature で公開型に serde 導出（`serde_impl.rs`）と JSON DTO を用意する（§18.5）。
8. `ffi.rs`（`ffi` feature）で `extern "C"` 層・解放関数・`catch_unwind` を実装する（§18）。
9. `cbindgen.toml` を追加し `include/dvorakj_parser.h` を生成する。
10. `examples/c_consumer` と `tests/ffi_roundtrip.rs` を追加する。
11. sloth 側に `RmapDvorakJLayoutLoader` adapter を追加する。
12. `sloth-daemon/src/main.rs` の loader 登録先を adapter に変更する。
13. fixture test で独立化前後の `sloth_core::Layout` 互換性を確認する。
14. `cargo tree -p dvorakj-parser --no-default-features` で rlib default の依存ゼロを確認する。
15. `cargo build -p dvorakj-parser --features cdylib-full` で DLL/SO 生成を確認する。

## 15. 受け入れ条件

### 15.1 rlib（依存ゼロ）

- `dvorakj-parser/src/` に `sloth_core::` 参照が存在しない。
- default feature の dependencies が空である（`cargo tree --no-default-features` が dvorakj-parser 単体）。
- `cargo test -p dvorakj-parser --no-default-features` が成功する。
- `parse_str` は `&str` 入力だけで DvorakJ layout を `ParsedLayout` に変換できる。
- `strict=false` で現状と同じ寛容パースを行い、未知要素は `warnings` に記録する。

### 15.2 DLL/SO（単体利用）

- `cargo build -p dvorakj-parser --features cdylib-full` で `dvorakj_parser.dll`（Windows）/ `libdvorakj_parser.so`（Linux）/ `libdvorakj_parser.dylib`（macOS）が生成される。
- DLL/SO 単体（`sloth-core` なし）で、`.jp.txt` の Shift-JIS バイト列を渡して JSON を取得できる。
- C の最小プログラムから `LoadLibrary`/`dlopen` → `dvorakj_parse_json` → `dvorakj_string_free` が動作する。
- `cbindgen` で `include/dvorakj_parser.h` が生成できる。
- 不正入力（無効 UTF-8、壊れたブロック、巨大入力）でも DLL がクラッシュせず、エラーコードを返す（`catch_unwind` で panic を封じる）。
- `tests/ffi_roundtrip.rs` が成功する。

### 15.3 sloth 統合

- `sloth` 側 adapter 経由で既存 `.en.txt` / `.jp.txt` layout が従来どおり動く。
- `config.toml` 形式の sloth 独自レイアウトは本設計の対象外とし、別設計で扱う。

## 16. リスクと対策

| リスク | 影響 | 対策 |
|--------|------|------|
| `Key` と `sloth_core::KeyCode` の分岐 | 変換漏れ | exhaustive `match` とテストで検出 |
| Shift-JIS を default で扱えない | `.jp.txt` 利用者が戸惑う | optional `encoding` feature または adapter 側 decode を明記 |
| 現状の silent skip が外部利用者には分かりづらい | 入力ミスの発見が遅れる | `ParseWarning` を追加し、strict mode を用意 |
| `BTreeMap` 化による挙動差 | 比較順序や snapshot 差分 | 意味論比較テストを用意 |
| `sloth-core` と parser の型が重複 | メンテ負荷 | variant 名を揃え、adapter test を必須化 |
| FFI ABI 破壊 | DLL 利用者のバイナリ互換が壊れる | 直接 struct を公開せず JSON/opaque handle を基本にし、関数名と戻り値を安定化 |
| Rust panic が FFI 境界を越える | UB / プロセス異常終了 | すべての `extern "C"` を `catch_unwind` で包み、panic をエラーコードに変換 |
| Rust allocator で確保したメモリを利用側が解放 | UB / heap corruption | 返却文字列は必ず `dvorakj_string_free` で解放する契約にする |
| JSON map key 問題 | `BTreeMap<KeyChord, _>` をそのまま JSON にできない | FFI/JSON DTO では map を list of entries に変換 |
| DLL の依存 DLL 不足 | 配布先でロード失敗 | `cdylib-full` に Rust crate 依存を静的に取り込み、C ランタイム依存を配布手順に明記 |

## 17. 推奨する公開面

最初の公開 API は小さく保つ。

```rust
pub use model::{
    Key,
    KeyChord,
    KeyboardLayout,
    LayoutMode,
    InputMode,
    Modifiers,
    OutputSeq,
    OutputToken,
    ParsedLayout,
    ParseError,
    ParseOptions,
    ParseReport,
    ParseWarning,
    SpecialKey,
};

pub fn parse_str(text: &str, options: ParseOptions) -> ParseResult<ParseReport>;
```

`DvorakJLayoutLoader` という sloth 固有名は公開 parser crate から外す。外部向けには `parse_str` と `DvorakJParser` のみを基本入口にする。

## 18. FFI / cdylib 層（DLL/SO 単体利用）

DLL/SO 単体で利用する非 Rust 呼び出し側（C/C++/C#/Python/Node など）のための C ABI 層。`ffi` feature 有効時のみコンパイルされる。

### 18.1 設計原則

- Rust の型（`&str`, `Result`, `Vec`, `BTreeMap`, データ付き enum）は C ABI を越えられないため、境界では **プリミティブ（ポインタ + 長さ + 整数コード）と JSON 文字列**だけを使う。
- 複雑な `ParsedLayout` は **UTF-8 JSON 文字列**として返す。これが最も可搬で、ABI 破壊に強い。
- Rust が確保したメモリは Rust が解放する。返却したポインタは必ず対の解放関数で返す。
- すべての `extern "C"` 関数は `catch_unwind` で panic を封じ、エラーコードへ変換する。

### 18.2 crate 設定

```toml
[lib]
crate-type = ["rlib", "cdylib"]
```

- `rlib`: `sloth` 側や他 Rust crate から通常依存で使う。
- `cdylib`: `dvorakj_parser.dll` / `libdvorakj_parser.so` / `libdvorakj_parser.dylib` を生成。

DLL/SO 配布ビルドは `--features cdylib-full`（= `ffi` + `encoding`）。

### 18.3 エラーコード

```rust
#[repr(C)]
pub enum DjStatus {
    Ok = 0,
    NullPointer = 1,
    InvalidUtf8 = 2,
    DecodeError = 3,
    ParseError = 4,
    Panic = 5,
    SerializeError = 6,
}
```

`ParseError`/`ParseWarning` の詳細は可能な限り JSON 出力側に含める。`DjStatus::Ok` では parse report JSON、`DjStatus::ParseError` では error JSON を `out_json` に返す。NULL pointer / panic / serialize error 等、JSON を安全に構築できない場合は `out_json` を NULL のままにできる。C 側は `DjStatus` で成否を判定し、追加情報が必要なら JSON を読む。

### 18.4 C ABI 入口

```rust
/// バイト列（.txt ファイル内容）を受け取り、ParseReport を JSON 文字列で返す。
/// - bytes/len: 入力ファイルのバイト列（Shift-JIS/UTF-8/BOM を内部判定）
/// - source_id: NUL 終端 C 文字列（拡張子で keyboard/decode を決定）。NULL 可。
/// - strict: 0 = 寛容, 非0 = strict
/// - out_json: 成功/パースエラー時に呼び出し側が dvorakj_string_free で解放する UTF-8 JSON
///   （Ok は report JSON、ParseError は error JSON。NULL の場合あり）
/// 戻り値: DjStatus
///
/// この関数はバイト列デコードのため `encoding` feature を必須とする。
/// `ffi` は `encoding` を含まないので、下の cfg で二重ゲートする。
/// `encoding` 無しビルドでは本関数はコンパイルされず、`dvorakj_parse_str_json`
/// のみが公開される。
#[cfg(feature = "encoding")]
#[no_mangle]
pub extern "C" fn dvorakj_parse_json(
    bytes: *const u8,
    len: usize,
    source_id: *const c_char,
    strict: c_int,
    out_json: *mut *mut c_char,
) -> DjStatus;

/// デコード済み UTF-8 テキストを直接渡す版（encoding 不要ケース）。
#[no_mangle]
pub extern "C" fn dvorakj_parse_str_json(
    text: *const u8,
    len: usize,
    source_id: *const c_char,
    strict: c_int,
    out_json: *mut *mut c_char,
) -> DjStatus;

/// dvorakj_parse_* が返した文字列を解放する。
#[no_mangle]
pub extern "C" fn dvorakj_string_free(s: *mut c_char);

/// ライブラリのバージョン文字列（静的、解放不要）。
#[no_mangle]
pub extern "C" fn dvorakj_version() -> *const c_char;

/// ABI バージョン（構造/JSON schema の後方互換判定用）。
#[no_mangle]
pub extern "C" fn dvorakj_abi_version() -> u32;
```

`dvorakj_parse_json` は `encoding` feature を要するため `#[cfg(feature = "encoding")]` でゲートし、実質 `cdylib-full` 前提となる（`ffi` 単体では公開されない）。`dvorakj_parse_str_json` は `encoding` 無し（`ffi` 単体）でもコンパイル・動作する。§18.5 の実装スケッチも同じ cfg でゲートすること。

### 18.5 実装スケッチ

```rust
#[cfg(feature = "encoding")] // decode::parse_bytes に依存するため
#[no_mangle]
pub extern "C" fn dvorakj_parse_json(
    bytes: *const u8,
    len: usize,
    source_id: *const c_char,
    strict: c_int,
    out_json: *mut *mut c_char,
) -> DjStatus {
    if bytes.is_null() || out_json.is_null() {
        return DjStatus::NullPointer;
    }
    let result = std::panic::catch_unwind(|| {
        let buf = unsafe { std::slice::from_raw_parts(bytes, len) };
        let sid = unsafe { c_str_to_string(source_id) }.unwrap_or_default();
        let opts = ParseOptions {
            source_id: Some(sid.clone()),
            keyboard: KeyboardLayout::from_source_id(&sid),
            strict: strict != 0,
        };
        crate::decode::parse_bytes(buf, &sid, opts)
    });

    match result {
        Ok(Ok(report)) => {
            let dto = JsonReport::from(report); // KeyChord map -> list 変換
            match serde_json::to_string(&dto) {
                Ok(s) => match string_into_c(s) {
                    Some(ptr) => {
                        unsafe { *out_json = ptr };
                        DjStatus::Ok
                    }
                    None => DjStatus::SerializeError, // 内部 NUL
                },
                Err(_) => DjStatus::SerializeError,
            }
        }
        Ok(Err(parse_err)) => {
            let err = JsonError::from(parse_err);
            if let Ok(s) = serde_json::to_string(&err) {
                if let Some(ptr) = string_into_c(s) {
                    unsafe { *out_json = ptr };
                }
            }
            DjStatus::ParseError
        }
        Err(_panic) => DjStatus::Panic,
    }
}

/// String -> *mut c_char。内部 NUL を含むと `CString::new` が失敗する。
/// serde_json 出力は制御文字をエスケープするため通常 NUL を含まないが、
/// 万一含む場合は空文字を返して Ok を偽装せず、呼び出し側で
/// `SerializeError` を返せるよう `Option` で失敗を伝える。
fn string_into_c(s: String) -> Option<*mut c_char> {
    std::ffi::CString::new(s).ok().map(|c| c.into_raw())
}

#[no_mangle]
pub extern "C" fn dvorakj_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(s)) };
    }
}
```

JSON DTO では `BTreeMap<KeyChord, _>` を map キーにできないため、entries のリストへ変換する。

```jsonc
{
  "ok": true,
  "schema_version": 1,
  "layout": {
    "name": "...",
    "mode": "Mixed",
    "keyboard": "Jis",
    "single_map": [ { "key": "A", "output": [ ... ] } ],
    "combos": [ { "keys": ["Space", "A"], "output": [ ... ] } ],
    "layer_maps": [ { "keys": ["Space"], "map": [ { "key": "A", "output": [...] } ] } ]
  },
  "warnings": [ { "type": "UnknownTrigger", "value": "zz", "line": 12 } ]
}
```

Parse error 時:

```jsonc
{
  "ok": false,
  "schema_version": 1,
  "error": { "type": "UnknownTrigger", "value": "zz", "line": 12 }
}
```

### 18.6 メモリ・スレッド契約

- `dvorakj_parse_*` が `out_json` に書いたポインタは、必ず `dvorakj_string_free` で解放する。それ以外の `free`/`delete` は UB。
- 返却ポインタは NUL 終端 UTF-8。長さが必要なら C 側で `strlen`。
- パーサは状態を持たず、関数はスレッドセーフ（引数のバッファを呼び出し中に他スレッドが書き換えないこと）。
- `dvorakj_version` の戻り値は静的領域で、解放してはならない。

### 18.7 ヘッダ生成（cbindgen）

`cbindgen.toml`:

```toml
language = "C"
include_guard = "DVORAKJ_PARSER_H"
autogen_warning = "/* Generated by cbindgen. Do not edit. */"

[enum]
prefix_with_name = true   # 変数名を DjStatus_Ok 等にして衝突回避
```

C 側で `DjStatus` / `DjStatus_Ok` を得るには、**Rust 型名と cbindgen prefix の二重付与を避ける**必要がある。`[export] prefix` は Rust 名の**前**にさらに付くため、Rust 側を `DjStatus` としたまま `prefix = "Dj"` を設定すると `DjDjStatus` になってしまう。次のいずれかに統一する。

- **方針A（推奨）**: Rust 型名を `DjStatus` のまま使い、`[export] prefix` は**設定しない**。関数は `#[no_mangle]` 名を維持。
- 方針B: Rust 型名を `Status` とし `[export] prefix = "Dj"` を設定して cbindgen に前置させる。

本設計は方針A を採り、`#[repr(C)] pub enum DjStatus` の名前をそのまま C ヘッダに出す。

生成:

```sh
cbindgen --config cbindgen.toml --crate dvorakj-parser --output include/dvorakj_parser.h
```

C 利用者はこのヘッダと DLL/SO をリンク、または実行時 `dlopen`/`LoadLibrary` する。

### 18.8 C 側最小利用例

```c
#include "dvorakj_parser.h"
#include <stdio.h>

int main(void) {
    unsigned char *buf; long n;
    /* buf = read "layout.jp.txt" bytes, n = size */
    char *json = NULL;
    DjStatus st = dvorakj_parse_json(buf, (size_t)n, "layout.jp.txt", 0, &json);
    if (json) {
        printf("%s\n", json);
        dvorakj_string_free(json);
    }
    if (st != DjStatus_Ok) {
        fprintf(stderr, "parse failed: %d\n", (int)st);
    }
    return 0;
}
```

### 18.9 パニック安全性

- パーサ内部の `format!("{:?}", k)`（canonical sort tie-break）や文字列スライスは、マルチバイト境界で panic しうる。`char` 単位イテレーションを用いる現行実装は概ね安全だが、FFI 層は保険として `catch_unwind` を必須にする。
- `catch_unwind` を機能させるため、cdylib プロファイルは `panic = "unwind"`（default）を維持する。`panic = "abort"` にすると `catch_unwind` が無効化されるため、cdylib-full ビルドでは abort を設定しない。

### 18.10 FFI テスト

- `tests/ffi_roundtrip.rs`: `ffi` feature を有効化し、`dvorakj_parse_str_json` → JSON パース → 主要フィールド検証 → `dvorakj_string_free` を Rust 側から呼ぶ。
- NULL ポインタ、空入力、無効 UTF-8、巨大入力で `DjStatus` が期待どおり返り、クラッシュしないことを確認。
- 可能なら `examples/c_consumer` を CI でビルド・実行し、実バイナリ経路も検証する。
