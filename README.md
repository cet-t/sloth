```
██████╗ ███╗   ███╗ █████╗ ██████╗
██╔══██╗████╗ ████║██╔══██╗██╔══██╗
██████╔╝██╔████╔██║███████║██████╔╝
██╔══██╗██║╚██╔╝██║██╔══██║██╔═══╝
██║  ██║██║ ╚═╝ ██║██║  ██║██║
╚═╝  ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═╝
```

キーボードリマッパー

- [x] 基本的なレイアウトシステム（DvorakJ互換パーサ）
- [x] SandS (Space and Shift)
- [x] 同時打鍵エンジン
- [ ] 打鍵（タップキー）
- [x] 前置シフト（prefix shift）
- [ ] 後置シフト（suffix shift）
- [x] 連続シフト（sequential shift）
- [ ] per-app プロファイル切り替え
- [ ] macOS / Linux 対応

## 課題・既知の問題

- DPI スケーリング: モニター間移動時のウィンドウ膨張を修正済み（Win32 API で物理サイズ固定）
- `prefix_window_ms` は config に残存するが matcher では未使用
- `prefix_since` は setter のみ（read なし）= dead code
- `config.json` の相対パス解決は `resolve_layout_path()` で自動付与（`data/` プレフィックス）

## ロードマップ

- 打鍵（タップキー）の実装
- 後置シフト（suffix shift）の実装
- per-app プロファイル切り替えの実装
- macOS / Linux 対応
