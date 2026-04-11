<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.3.0（2026-04-11）](#v0302026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.4.0（2026-04-11）

ログビュアー機能を別ウインドウで追加し、「ユーザの明示操作とパニック時以外はログファイルを生成しない」方針へ全面的に切り替えた。

### 新機能

- **ログビュアー（OS 別ウインドウ）** — トップバーの「ログ」ボタンが、インメモリのログバッファをリアルタイムに表示する独立した OS ウインドウを開く動作に変わった。`eframe` の `show_viewport_deferred` を採用しているため、メインの 3D ビューポートとは完全に独立して動き、別モニタへの移動・個別の最小化が可能で、新しいログが流入してもメイン 3D シーンを再描画させない（deferred クロージャ内で約 150ms ポーリング）。
- **レベルフィルタ** — Error / Warn / Info / Debug をチェックボックスで個別に切替。色分け（Error=赤、Warn=黄、Info=白、Debug/Trace=灰、Unknown=薄灰）。バックトレース等のマルチラインメッセージは 1 件として連結して扱う。
- **自動追尾** — トグル ON で末尾スクロールに追従。手動でスクロールアップすると一時停止し、最下部へ戻すと再開。
- **ログの手動保存** — 「ログ保存」ボタンで `rfd::FileDialog` 経由で任意のパスへ `.log` を書き出し。「フォルダを開く」ボタンで `logs/` ディレクトリをエクスプローラで開く。
- **状態の永続化** — ログビュアーの表示状態、ウインドウ位置・サイズ、レベルフィルタ設定を `popone.toml` の `[log_viewer]` セクションに保存し、次回起動時に復元する。

### 挙動の変更

- **通常終了時にログファイルを自動生成しなくなった。** 従来は通常終了時にインメモリログを `popone_<ts>.log` へフラッシュしていたが、v0.4.0 ではこれを廃止。バッファはメモリに残ったままプロセス終了で破棄される。セッションのログを残したい場合は新設の「ログ保存」ボタンを使う。
- **パニックダンプを `panic_<ts>.log` へ直接書き込み。** 従来は「`popone_<ts>.log` に書いてから `panic_<ts>.log` にコピー」という経路でクラッシュごとに 2 ファイル生成していたが、`panic_<ts>.log` を直接書き出す形に変更し、1 クラッシュ＝1 ファイルに揃えた。
- **ログ自動ローテーションを廃止。** `rotate_logs` と `[log] keep` 設定を削除。`%LOCALAPPDATA%\popone\logs\` 配下のファイルは「ユーザの明示操作（手動エクスポート）またはパニック」の結果のみ存在するようになったため、自動削除すべきではないという判断。既存 `popone.toml` の `[log] keep = N` 行は serde が未知フィールドを無視するため、後方互換性は維持される。

### 内部実装

- 新規モジュール `popone/src/viewer/log_viewer.rs` を追加。`[HH:MM:SS.mmm][LEVEL] message` 形式の手書きパーサ、リングバッファ（20,000 行上限）、インクリメンタル更新の `filter_indices`、17 件のユニットテスト（マルチライン連結、バイト単位 drain による先頭断片破棄、CRLF、レベルフィルタ往復、起動時/同セッション reopen での geometry 復元など）を含む。
- `LogViewerModel` は `Arc<Mutex<LogViewerModel>>` で保持し、`show_viewport_deferred` のクロージャ（`Fn + Send + Sync + 'static` 制約あり）に `Arc::clone` で渡せるようにした。
- ウインドウの位置・サイズは毎フレーム子 viewport の入力からキャプチャし、同セッション内の開閉・プロセス再起動の両方で位置が正しく往復するよう実装した。

## v0.3.0（2026-04-11）

初回公開リリース。ドキュメント MECE 再編、UX 改善、UnityPackage 周りのバグ修正を中心に構成。
