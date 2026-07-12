# sloth コード署名 設計・手順書

## 1. 目的・背景

現状の `build-release.ps1` は Windows 用 ZIP を手動で `gh release` 配布している。
その中の実行ファイル（`.exe`）は**コード署名されていない**ため、Windows では
SmartScreen の「保護された PC」警告が出る。

「中身をユーザー環境で生成する」以外のアプローチとして、**無料のコード署名**
で OS に「認証済み」と見なさせる構成を採用する。

> 前提の正確な理解：インストーラを作るだけでは実行ファイルの署名にはならない。
> 実行ファイルそのものを署名する必要がある。

## 2. プラットフォーム別の現実

| OS | OS信頼に必要なもの | 無料で可能か |
|----|--------------------|--------------|
| Windows | Authenticode 署名（信頼済みCA証明書） | **SignPath(OSS) で無料可** |
| macOS | Apple Developer ID 証明書 + 公証(notarization) | **不可（$99/年必須）** |
| Linux | OSレベルの署名は不要 | 不要（GPG/cosign で整合性保証は無料） |

### 重要な注意
- **自己署名証明書 / Let's Encrypt / Sigstore は Windows SmartScreen を通せない。**
  SmartScreen は「信頼済みCA루트証明書」で検証するため、個人が発行した証明書では
  警告が残る。
- **Sigstore cosign** は supply-chain の整合性・ provenance 保証には優秀だが、
  Windows/macOS の「OS信頼（SmartScreen/Gatekeeper）」には寄与しない。
  あくまで「ダウンロードしたものが改ざんされていないか」の検証用。
- macOS を完全に信頼させるには将来的に Apple Developer Program への登録が必要。
  それまでは未署名 + 手動実行許可の案内とする。

## 3. 採用方式

- **Windows**：[SignPath.io](https://signpath.io) の OSS 無料枠で Authenticode 署名。
  - ビルド成果物（`.exe`）を SignPath に送り、同社の信頼済み証明書で署名して返す。
  - ユーザー側では SmartScreen 警告が出ない。
- **全平台**：[Sigstore cosign](https://github.com/sigstore/cosign)（keyless, GitHub OIDC）
  でリリース成果物に署名・検証メタデータを付与。OS信頼ではなく整合性保証。
- **配布・ビルド基盤**：`cargo-dist` でマルチプラットフォームビルドと
  GitHub Releases 連携、かつ SignPath/cosign ステップを組み込む。
- **macOS**：Phase 2 で有料 Apple Developer を検討（本書では Phase 1 対象外）。

## 4. SignPath 登録手順（ユーザーが実施）

1. https://signpath.io でアカウント作成（GitHub ログイン可）。
2. Organization を作成し、リポジトリ `cet-t/sloth` を OSS プロジェクトとして登録。
   - OSS 無料枠の条件（公開リポジトリ等）を満たすことを確認。
3. Signing Policy を作成：
   - Signing method: `Authenticode`
   - Artifact 対象: `*.exe`（必要に応じ `*.msi`）
   - 許可するビルド元: GitHub Actions の OIDC（組織/リポジトリを制限）
4. CI 用の **SignPath API token** を発行（Settings → API tokens）。
5. 発行された `Submitter token` / `Signing token` を控える。

## 5. 必要な GitHub Secrets / Variables

| 名前 | 用途 | 設定元 |
|------|------|--------|
| `SIGNPATH_API_TOKEN` | SignPath 署名送信 | SignPath の API token |
| `SIGNPATH_ORGANIZATION_ID` | SignPath 組織ID | SignPath ダッシュボード |
| `SIGNPATH_PROJECT_SLUG` | プロジェクト識別子 | SignPath ダッシュボード |
| `SIGNPATH_POLICY_NAME` | 署名ポリシー名（例: `release-signing`）| SignPath ダッシュボード |

- cosign はキー管理不要（GitHub OIDC で keyless 署名）。シークレット不要。
- これらは **Settings → Secrets and variables → Actions** に登録。

## 6. GitHub Actions アーキテクチャ（案）

```
on: push (tag v*)

jobs:
  build:                       # cargo-dist が生成
    - 各 target で cargo build --release
    - Windows: .exe を成果物として upload
  sign-windows:
    - needs: build
    - Windows .exe を download
    - SignPath API へ submit（OIDC 認証）
    - 署名済み .exe を download し upload
  publish:
    - needs: [build, sign-windows]
    - GitHub Release 作成 + cosign で成果物署名
```

- 実装済: `.github/workflows/release.yml`
  - `build` ジョブ（matrix）: `cargo-bins/cargo-dist@v0.9.0` でビルド →
    Windows のみ `actions/upload-artifact@v4` で未署名アーティファクトを上げ、
    `signpath/github-action-submit-signing-request@v2` で Authenticode 署名 →
    署名済みを `dist-<target>` として再 upload。
  - `release` ジョブ: 成果物を download → `sigstore/cosign-installer` で
    keyless(OIDC) 署名（`.cosign`）→ `softprops/action-gh-release@v2` でリリース作成。
  - cargo-dist の `hosting=[]` により自動リリースは無効化し、署名ステップを
    挟んでから自前でリリースする構成。
- SignPath のアーティファクト設定雛形: `.signpath/artifact-configurations/default.xml`
  （`<zip-file>` ルートで `sloth.exe` / `sloth-config.exe` を Authenticode 署名）。
- 既存 `build-release.ps1` は本ワークフロー導入後に廃止推奨（手動リリース不要に）。

## 7. ロールアウト手順（段階的）

1. **Phase 0（設計確定）**：本書の合意。← いまここ
2. **Phase 1**：SignPath 登録（ユーザー）＋ Secrets 設定。
3. **Phase 2（実装済）**：`release.yml` / `.signpath/default.xml` / Cargo.toml メタデータを作成済み。
   残作業はユーザー側の SignPath 登録＋Secrets 設定。その後 Windows のみで
   タグ push テストし、ローカルで SmartScreen 警告が消えることを確認。
4. **Phase 3**：macOS/Linux target を追加、cosign 署名を有効化。
5. **Phase 4（将来）**：macOS 用 Apple Developer 登録・notarization を追加。

## 8. 検証手手順

- Windows: 署名済み `.exe` を別マシン/VM でダウンロード実行し、
  SmartScreen 警告が出ないことを確認。`Get-AuthenticodeSignature .\sloth.exe` で
  署名者が SignPath になっていることを確認。
- 全平台: `cosign verify-blob` でリリース成果物の整合性を検証できることを確認。

## 9. 未解決・注記

- macOS の無料信頼手段は存在しない（Apple Developer $99/年が必要）。
- SignPath OSS 無料枠の利用上限（署名回数等）を登録時に要確認。
- 自己署名や Sigstore 単体では SmartScreen は回避できない点をステークホルダーへ共有。
