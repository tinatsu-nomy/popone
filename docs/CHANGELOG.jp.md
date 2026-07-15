<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.5.16（2026-07-15）](#v05162026-07-15)
    - [改善](#%E6%94%B9%E5%96%84)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD)
  - [v0.5.15（2026-07-14）](#v05152026-07-14)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-1)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98)
  - [v0.5.14（2026-07-08）](#v05142026-07-08)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-2)
    - [バグ修正（本リリース内の追加修正）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E6%9C%AC%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%86%85%E3%81%AE%E8%BF%BD%E5%8A%A0%E4%BF%AE%E6%AD%A3)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-1)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-1)
  - [v0.5.13（2026-07-06）](#v05132026-07-06)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-1)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-2)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-2)
  - [v0.5.12（2026-06-11）](#v05122026-06-11)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-2)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-1)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-3)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-3)
  - [v0.5.11（2026-05-16）](#v05112026-05-16)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-3)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-4)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-2)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-4)
  - [v0.5.10（2026-05-15）](#v05102026-05-15)
    - [バグ修正](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3-3)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-3)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-5)
  - [v0.5.9（2026-05-05）](#v0592026-05-05)
    - [新機能 / 改善](#%E6%96%B0%E6%A9%9F%E8%83%BD--%E6%94%B9%E5%96%84)
    - [内部実装（i18n 整備）](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85i18n-%E6%95%B4%E5%82%99)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-6)
  - [v0.5.8（2026-04-22）](#v0582026-04-22)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-4)
  - [v0.5.7（2026-04-22）](#v0572026-04-22)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-4)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-5)
  - [v0.5.6（2026-04-14）](#v0562026-04-14)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-5)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-6)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C)
  - [v0.5.5（2026-04-13）](#v0552026-04-13)
    - [新機能 (Phase 1)](#%E6%96%B0%E6%A9%9F%E8%83%BD-phase-1)
    - [新機能 (Phase 2)](#%E6%96%B0%E6%A9%9F%E8%83%BD-phase-2)
    - [新機能 (Phase 3)](#%E6%96%B0%E6%A9%9F%E8%83%BD-phase-3)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-7)
    - [スコープ注記](#%E3%82%B9%E3%82%B3%E3%83%BC%E3%83%97%E6%B3%A8%E8%A8%98-7)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-1)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-5)
  - [v0.5.4（2026-04-13）](#v0542026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-6)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-8)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-2)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-6)
  - [v0.5.3（2026-04-13）](#v0532026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-7)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-9)
  - [v0.5.2（2026-04-13）](#v0522026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-8)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-10)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-3)
  - [v0.5.1（2026-04-13）](#v0512026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-9)
    - [パフォーマンス](#%E3%83%91%E3%83%95%E3%82%A9%E3%83%BC%E3%83%9E%E3%83%B3%E3%82%B9)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-11)
    - [バグ修正（リリース前レビュー対応）](#%E3%83%90%E3%82%B0%E4%BF%AE%E6%AD%A3%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9%E5%89%8D%E3%83%AC%E3%83%93%E3%83%A5%E3%83%BC%E5%AF%BE%E5%BF%9C-4)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-7)
    - [v0.6.0 に延期](#v060-%E3%81%AB%E5%BB%B6%E6%9C%9F)
  - [v0.5.0（2026-04-13）](#v0502026-04-13)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-10)
    - [挙動の変更](#%E6%8C%99%E5%8B%95%E3%81%AE%E5%A4%89%E6%9B%B4)
    - [テスト](#%E3%83%86%E3%82%B9%E3%83%88-8)
  - [v0.4.0（2026-04-11）](#v0402026-04-11)
    - [新機能](#%E6%96%B0%E6%A9%9F%E8%83%BD-11)
    - [挙動の変更](#%E6%8C%99%E5%8B%95%E3%81%AE%E5%A4%89%E6%9B%B4-1)
    - [内部実装](#%E5%86%85%E9%83%A8%E5%AE%9F%E8%A3%85-12)
  - [v0.3.0（2026-04-11）](#v0302026-04-11)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.5.16（2026-07-15）

### 改善

- **選択ダイアログ表示中のロード要求を拒否** — アーカイブ内モデル選択・UnityPackage モデル選択・FBX 読み込み方法・OBJ/STL インポート設定・アーカイブパスワード・テクスチャ手動割当の各ダイアログが選択待ちの間は、D&D、「開く」「追加」ボタン、2 重起動（シングルインスタンス連携）、リロードによる新規ロード要求をトースト表示付きで拒否するようにした。従来はダイアログの裏で別のロードが走り、選択待ち状態が上書きされて不定な動作になり得た。

### 新機能

- **アーカイブ内テキストビューワ** — ZIP / 7z / RAR に同梱されたテキストファイル（`.txt` / `.md`、readme や利用規約など）を、モデルファイルとは別の一覧として表示できるようになった。テキストがある場合のみトップバーに「テキスト (n)」ボタンが現れ、一覧ウインドウのファイルをクリックすると内容を別ウインドウ（OS ウインドウ）で表示する。複数ファイルを同時に開ける。複数モデル選択ダイアログにも同じ一覧が表示されるため、readme を読んでから読み込むモデルを選べる。文字コードは UTF-8（BOM あり / なし）・UTF-16（BOM）・Shift_JIS を自動判定し、内側アーカイブ（多段 1 段）内のテキストも対象。一覧はアーカイブの読み込みごとに置き換わり（アペンド時は追加）、通常のモデルファイルを読み込んだ後も次のアーカイブ読み込みまで保持される。内側アーカイブがパスワード付きでリストに失敗した場合も、外側のテキスト（パスワードが記載されていることの多い readme 等）はパスワードダイアログと同時に一覧へ表示される。1 ファイル 4 MB 超は対象外、表示は先頭 100 万文字まで。

## v0.5.15（2026-07-14）

アーカイブ機能拡張リリース。RAR アーカイブ対応と、パスワード付きアーカイブ対応（GUI のみ）を追加した。

### 新機能

- **RAR アーカイブ対応** — `.rar` ファイル（RAR4 / RAR5）を、ZIP / 7z が対応済みだったすべての経路（ビューアのファイルダイアログ、ドラッグ＆ドロップ、アペンド〔Shift + ドロップ〕、CLI 変換 / `--list-models`）で開けるようになった。展開には公式 UnRAR ライブラリ（`unrar` crate 経由で静的リンク）を使用。7z と同様にソリッド形式のため、対象拡張子のエントリを 1 パスで全展開する（合計 2 GB 上限は共通）。UnRAR のライセンスは `THIRD_PARTY_NOTICES.md` を参照。
- **パスワード付きアーカイブ対応（GUI のみ）** — 暗号化された ZIP（ZipCrypto / AES）・7z（ヘッダ暗号化含む）・RAR（ヘッダ暗号化含む）を開くと、汎用エラーで失敗する代わりにビューアがパスワード入力ダイアログを表示するようになった。パスワードが違う場合はエラー表示付きでダイアログが再表示される。ZIP は展開段階で復号するため、入力前に選択していたモデルはリトライ後に自動で再選択される。**パスワードはリトライするロードの間だけメモリに保持され、`popone.toml` への書き込み・ログ出力・ロードをまたいだ記憶は一切行わない。** CLI には意図的にパスワード入力機能を設けず、暗号化アーカイブは GUI への案内メッセージ付きでエラーになる。
- **多段アーカイブ対応（1 段）** — アーカイブの中に別のアーカイブ（ZIP / 7z / RAR の任意の組み合わせ）が入っている場合、内側アーカイブを展開してそのモデルを合成パス `外側/内側.zip/モデル.pmx` でモデル選択一覧に合流させるようにした。「readme と（パスワード付きのことが多い）内側 ZIP を同梱」という MMD 配布の定番形式がそのまま読み込める。パスワード入力時は同じパスワードが内側アーカイブにも適用される（平文エントリはパスワードを無視）。展開は意図的に 1 段まで（自己参照アーカイブ爆弾への防御）。内側アーカイブが壊れている場合は警告ログを出してスキップし、外側の一覧は使える状態を保つ。

### バグ修正

- **モーフ畳み込みメッシュ読み込み時のパニック（mikktspace の境界外アクセス）** — 全頂点が同一点にあるサブメッシュ（隠しパーツを 1 点に畳んで頂点モーフで展開する MMD の常套手法）を含む PMX 等を読み込むと、`mikktspace-0.2.0` 内部（`GenerateSharedVerticesIndexList`）で `index out of bounds` パニックが発生し BG ロードスレッドが落ちていた（座標の広がりがゼロだと頂点マージグリッドの除算が NaN になり全エントリがセル 0 に集中、空セル判定の前に次セルのオフセットがハッシュテーブル長ちょうどを指してしまう）。タンジェント生成前に「広がりゼロ / 非有限座標」のジオメトリを検出してデフォルトタンジェントを割り当てるようにし（位置の広がりがない以上タンジェントは無意味）、さらに `catch_unwind` の保険で mikktspace 移植内のその他のパニックも従来の「生成失敗」フォールバックに流すようにした。隠しメガネパーツが 1 点に畳まれた実モデルで修正を確認済み。
- **ソリッド 7z でスキップ対象ファイルがモデルより前にあると `ChecksumVerificationFailed` になる** — ソリッド 7z のブロックは 1 本の連続ストリームとして展開されるが、sevenz-rust2 の `BlockDecoder` は展開コールバックがスキップしたエントリの未読バイトを読み飛ばさない。popone のフィルタ展開はモデル・テクスチャ以外（`.vpd` ポーズファイル等）を読まずにスキップするため、スキップ対象がモデルより前にあるソリッドアーカイブでは以降の全エントリの読み取り位置がズレ、完全に正常なアーカイブ（7-Zip 本家では "Everything is Ok"）でも CRC 検証に失敗していた。スキップ時にストリームを最後まで読み捨てて位置を保つように修正。`Poses/` フォルダが PMX より前にある実在のソリッド 7z（LZMA2・1 ブロック）で修正を確認済み。
- **平文 ZIP エントリへのパスワード適用で展開が壊れる** — 暗号化されていないエントリに `by_index_decrypt` を使うと ZipCrypto 検証が平文データに対して走り失敗する。ヘッダが暗号化ありのエントリにだけパスワードを渡すように修正（暗号化・平文混在 ZIP や、内側アーカイブ用のパスワードを付けて平文の外側 ZIP を再読するケースで必要）。
- **バックグラウンドロード失敗がログに残らない** — ロードエラーは一時的なトースト表示のみで、見逃すとログファイルに痕跡が残らなかった（「ログが出ない」報告の原因）。失敗時に `ERROR` としてログにも出力するように修正。

### 内部実装

- `PoponeError` に `ArchivePasswordRequired` / `ArchiveBadPassword` を追加し、3 つのアーカイブバックエンド（zip / sevenz-rust2 / unrar）のパスワード系エラーをこの 2 種に正規化した。
- ZIP の一覧取得を `by_index` から `by_index_raw` に変更し、暗号化 ZIP でもパスワードなしで一覧（モデル選択）できるようにした（ZIP はエントリ本体のみ暗号化されるため）。
- AES 暗号化 ZIP 対応のため zip crate の `aes-crypto` feature を有効化した。

### テスト

- AES 暗号化 ZIP・ヘッダ暗号化 7z のパスワードラウンドトリップテスト（zip / sevenz-rust2 のライターでメモリ上に生成）：一覧・パスワード未指定・誤パスワード・正パスワードの各経路。
- unrar crate リポジトリ由来の小型フィクスチャ（平文・本体暗号化・ヘッダ暗号化）による RAR テスト：パスワード要求検出、正パスワードでのヘッダ復号、誤パスワード失敗、破損ファイル拒否。

### スコープ注記

- パスワード付きアーカイブのリロード時は再度パスワード入力が必要（記憶しない仕様のため）。CLI は暗号化アーカイブ非対応。
- RAR アーカイブの作成は非対応（UnRAR ライセンスでも禁止されている）。展開専用。

## v0.5.14（2026-07-08）

隠しオプション追加リリース。GUI トグルは存在せず、`popone.toml` を手動編集した場合のみ有効になる `[behavior]` セクションを新設した。

### 新機能

- **シングルインスタンス無効化オプション** — `popone.toml` に `[behavior] disable_single_instance = true` を追記すると、Named Mutex / Named Pipe による従来のシングルインスタンス制御自体をスキップし、複数の `popone` ウインドウを同時に起動できるようになる。既定は `false`（従来どおり）。
- **Escape キー終了オプション** — `popone.toml` に `[behavior] exit_on_escape = true` を追記すると、メインウインドウで Escape キーを押した際に即座に終了する（閉じるボタンと同等の `ViewportCommand::Close` を送信）。既定は `false`（従来どおり、ダイアログを閉じる用途の既存 Escape ハンドラのみが動作）。

### バグ修正（本リリース内の追加修正）

- **`disable_single_instance` 有効時の設定書き込み排他** — `popone.toml` / `popone_history.json` の保存ヘルパー `atomic_write()` は固定名の `.tmp`/`.bak` を使うため、書き込みプロセスが常に1つである前提に依存していた。これまでは Named Mutex による強制シングルインスタンスがその前提を保証していたが、`disable_single_instance = true` はその前提自体を崩す。複数ウインドウをほぼ同時に終了させると設定が失われたり壊れたりする恐れがあったため、`atomic_write()` 内部で別の Named Mutex（`Local\popone_viewer_config_write_lock`）を取得しプロセス間で書き込みを直列化するようにした。取得はタイムアウト付き（3秒）で、失敗時は警告ログを出しつつロックなしで書き込みを続行する。

### テスト

- **`BehaviorConfig` の後方互換性・ラウンドトリップテスト** — `[behavior]` セクションのない旧 `popone.toml` が両フラグ `false` として読み込まれること、および明示的な値が toml 往復で保持されることを検証。
- **並行書き込みの回帰テスト** — 8スレッドから同時に `save_config()` を呼び出しても `popone.toml` が破損せず、書き込み後に `.tmp`/`.bak` の残骸が残らないことを検証（Windows named mutex はスレッド間でもプロセス間と同じ規則で直列化されるため、実プロセス分離なしで排他ロックの経路を検証できる）。

### スコープ注記

- 両オプションとも GUI トグルはなく、`popone.toml` を手動編集しない限り既定値 `false` のままで従来と挙動は変わらない。

## v0.5.13（2026-07-06）

バグ修正リリース。ZIP／7z アーカイブから直接読み込んだ PMX で、参照ファイル名の大文字・小文字がアーカイブ内エントリと異なる場合（例：材質が `body.PNG` を参照しているのにアーカイブ内は `body.png`）でもテクスチャを解決できるようにした。これらのテクスチャが空データによる「フォーマットエラー」白フォールバックにならなくなる。

### バグ修正

- **アーカイブロード時の PMX テクスチャ照合を大文字小文字無視に** — PMX を ZIP／7z 内から読み込むと、テクスチャ参照はアーカイブのインメモリファイル（`aux_files`）と照合される。この照合が大文字・小文字を区別していたため、大文字小文字を区別しないファイルシステム（Windows／macOS）で作られた PMX が `textures/body.PNG` を参照しつつアーカイブ内エントリが `textures/body.png` の場合、バイト列を見つけられずテクスチャが空になり、画像デコードの「フォーマットエラー」（白フォールバック）として現れていた。アーカイブの**抽出**側は既に大文字小文字を無視して照合していたため実ファイルは正しい名前で `aux_files` に格納されており、`extract_textures` の**取り出し**側だけが厳密だった。取り出しに大文字小文字を無視するフォールバックを追加し、ZIP・7z 双方を修正。展開済みフォルダからの読み込みは OS がディスク上で大文字小文字を解決するため影響を受けていなかった。

### テスト

- **大文字小文字無視の aux 照合テスト** — `pmx/extract.rs` に `test_aux_get_ignore_case`（ヘルパー単位：拡張子・ディレクトリ名の大小差）と `test_extract_textures_case_insensitive_aux`（`Body_D.PNG` を参照する PMX がアーカイブ内 `body_d.png` の正しいバイト列を解決する）を追加。

### スコープ注記

- 出力フォーマットの変更なし。完全一致照合を先に試すため、大文字小文字が正しい参照は影響を受けず、大文字小文字無視はフォールバックとしてのみ働く。大文字小文字を区別するファイルシステム（Linux 等）でのディスクロードにおける大小不一致は、別件の未対応エッジケースとして残る。

## v0.5.12（2026-06-11）

バグ修正リリース。サブディレクトリにある `.mtl` を参照する OBJ で、その `.mtl` 内に書かれたテクスチャを `.obj` のディレクトリではなく `.mtl` 自身のディレクトリ基準で解決するようにした。サブディレクトリ構成の OBJ アセットでテクスチャが欠落しなくなる。

### バグ修正

- **MTL サブディレクトリのテクスチャ解決** — `.obj` が `mtllib sub/dir/model.mtl` のようにサブディレクトリの材質ライブラリを参照する場合、その `.mtl` 内のテクスチャ名（例 `map_Kd body.png`）は `.obj` ではなく `.mtl` からの相対パスである。従来はすべてのテクスチャを `.obj` のディレクトリ基準でのみ解決していたため、`.mtl` とテクスチャをサブディレクトリに置いた OBJ はテクスチャが欠落（白フォールバック）した状態で読み込まれていた。ローダが各 `mtllib` のディレクトリを記録し、`.obj` ディレクトリへフォールバックする前にそれを接頭辞として試すようにしたので、サブディレクトリ構成・フラット構成のどちらでも正しく解決される。

### 内部実装

- **ディスク経路とインメモリ経路の一本化** — `load_obj_with_params` がファイルを読み込んで `load_obj_from_data_with_params` に委譲するようにした。これによりディスクロードもアーカイブ／インメモリロードと同じ自前 `mtl_loader` クロージャを通る。ディスク経路が `.mtl` のディレクトリを捕捉できるのはこのためで、従来の `tobj::load_obj` の既定ローダはそれを破棄していた。

### テスト

- **OBJ の MTL サブディレクトリ解決テスト** — `obj/extract.rs` に `texture_resolves_relative_to_mtl_subdirectory`（サブディレクトリにネストした `.mtl` ＋テクスチャ）と `texture_resolves_in_flat_layout`（`.mtl` が `.obj` と同階層のケースの回帰テスト）を追加。いずれも一時ディレクトリに実ファイルを書き出し、解決されたテクスチャのバイト列を検証する。

### スコープ注記

- 出力フォーマットの変更なし。フラット構成の OBJ アセット（一般的なケース）は影響を受けない（`.obj` ディレクトリの探索はフォールバックとして残る）。

## v0.5.11（2026-05-16）

v0.5.10 の品質固めフォローアップ。PSD→PSB 自動切替について v0.5.10 が残していた検証の穴（サイズ推定の単体テストのみで、生成バイト列を読み戻していなかった）を埋め、シェーダーフラグメントの構文ミスがビューア起動時ではなく `cargo test` で検出されるよう WGSL コンパイルテストを自動化し、PSD→PSB 昇格を成功 toast で明示するようにした。出力フォーマットの変更はなく、通常モデルは従来どおりバイト単位で同一の `.psd` を書き出す。

### 新機能

- **UV マップ出力で PSD → PSB 自動切替時に明示 toast** — UV マップ writer が 1.9 GiB 閾値を超えて出力を `.psb` に切り替えた際、成功 toast が「レイヤーデータが PSD の 2 GiB 上限を超えたため PSD から自動切替した」旨を明示する専用メッセージを表示するようになった（ユーザーが要求していない `.psb` パスを黙って表示するのをやめた）。通常の `.psd` 出力は変更なし。`en` / `ja` / `zh` ロケールに `viewer.toast.uvmap.exported_psb` を追加。

### テスト

- **PSD/PSB ラウンドトリップ再パーステスト** — `convert/uvmap.rs` に、小キャンバスで PSD と PSB の両方を書き出して生成バイト列を再パースし、署名 / version / ヘッダ各フィールドと構造不変条件 `section_start + 宣言レイヤーセクション長 + image_data_len == file_len` を検証するテストを追加。この不変条件は長さフィールドがオーバーフローしたり誤った幅で書かれた瞬間に破綻する（まさに v0.5.10 の silent corruption の故障モード）ため、PSB コンテナがサイズ推定だけでなく構造的に開けることを保証する。
- **naga WGSL シェーダーコンパイルテスト** — `viewer/gpu.rs`（10 シェーダー）と `viewer/bloom.rs`（`BLOOM_SHADER_SRC`）のマクロ合成シェーダーソースを naga の WGSL front-end + Validator（wgpu と同じ front-end）に通す。`macro_rules!` フラグメントの構文ミスが、ビューア起動時ではなく `cargo test` で検出されるようになった。

### 内部実装

- **`naga` dev-dependency 追加** — `[dev-dependencies]` に `naga = { version = "24", features = ["wgsl-in"] }` を追加。`24` は `wgpu 24` が推移的に既に取り込んでいる `naga 24.0.0` に解決されるため、依存ツリーに naga のコピーは増えない。
- **CI が viewer ゲートのテストを実行** — `.github/workflows/ci.yml` に `cargo test --features viewer` ステップを追加。シェーダーコンパイルテストは `viewer` feature 下にあり、既存の `cargo test`（CLI のみ）ステップでは実行されなかった。

### スコープ注記

- 通常モデルの出力バイトに変更なし — `.psd` 出力は v0.5.10 とバイト単位で同一。ユーザーから見える変化は、PSB への自動昇格が発生した場合の toast 文言がより明示的になった点のみ。

## v0.5.10（2026-05-15）

**UV マップ PSD 出力 2 GiB silent failure** を解消するための、ピンポイントなバグ修正リリース。これまで高解像度・多材質のマージモデルを UV マップ出力すると、書き出し時はエラーが出ないのに Photoshop / Krita / Affinity / GIMP で開けない **破損 PSD** が生成されることがあった。本リリースから、推定レイヤーセクションサイズが PSD の `u32` 上限に近づいた場合、自動的に **PSB（Large Document Format / `.psb`）** 形式に切り替えて書き出すようになる。小〜中規模モデルは従来どおり `.psd` で書き出され、動作・出力結果に変化はない。

### バグ修正

- **UV マップ PSD 2 GiB silent corruption の解消** — 高解像度（4096 / 8192）× 多材質（20+）でマージしたモデルに対する UV マップ出力で、書き込み自体は成功するのに開けない `.psd` が生成される silent failure を解消。PSD 仕様では「Layer and Mask Information」セクション長が `u32`（最大約 2 GiB）として記録されるため、レイヤー数 × 解像度 × 4bpp が境界を超えるとファイルが破損する。本リリースから、書き込み前にレイヤーセクションサイズを推定し、保守的な閾値（1.9 GiB）を超えた場合に **PSB（Large Document Format）** に自動切替する。これに伴いシグネチャは `8BPB`、version は `2`、該当する section / channel データの長さフィールドは `u32` → `u64` に拡張され、出力ファイルの拡張子も `.psd` → `.psb` に書き換えられる。エクスポート API は実際に書き出されたパスを返すため、toast やログには実ファイル名が表示される。小〜中規模モデルは従来どおり `.psd` のまま出力され、出力バイナリはバイト単位で v0.5.9 と同一。

### 内部実装

- **`convert/uvmap.rs` に `PsFormat::Psd` / `PsFormat::Psb` enum を追加** — writer 全体にフォーマットフラグを伝搬する。PSD と PSB の構造差（外側「Layer and Mask Information」セクション長、内側「Layer Info」セクション長、チャンネル毎データ長の 3 箇所）は `write_section_length()` / `push_section_length()` ヘルパーに局所化されており、writer 本体はフォーマット中立を維持する。
- **`estimate_layer_section_bytes()` 予測ヘルパーを追加** — レイヤーセクションサイズを少し過大に見積もる（per-layer オーバーヘッドは 512 バイト切り上げ、Content レイヤーには `4 × (2 + pixel_count)`）。新規定数 `PSD_TO_PSB_THRESHOLD_BYTES = 1.9 GiB` と比較してフォーマットを決定する。
- **`export_uv_map_grouped()` の戻り値変更** — `io::Result<()>` から `io::Result<PathBuf>` に変更し、呼び出し側に実際に書き出されたパス（`.psb` への書き換え込み）を返す。`viewer/app/pending.rs` を新シグネチャに追従させ、成功 toast に実パスを表示するようにした。
- **テスト追加** — 拡張子書き換え（`.psd` ⇄ `.psb`）、PSD / PSB のヘッダバイト（`8BPS` + version 1 / `8BPB` + version 2）、length フィールド拡張による PSD と PSB の +24 バイト差分、レイヤーセクションサイズ予測の単調増加、現実的な負荷（4096 × 4096 × 30 layers で閾値超過、1 layer の 4 k は閾値以下）の境界判定の 6 件を新規ユニットテストとして追加。

### スコープ注記

- 通常モデルのファイル取り扱いに変更はなく、移行不要。v0.5.9 と同条件であれば出力 `.psd` はバイト単位で同一。
- PSB（`.psb`）は Photoshop CS / 2021+、Krita、Affinity Photo、GIMP（プラグイン経由）で対応されている。閾値は PSD 仕様上限（2 GiB）よりも保守的な 1.9 GiB に設定しており、推定器が緊密にバウンドできないレイヤーレコードオーバーヘッドの余裕を確保している。

## v0.5.9（2026-05-05）

`popone` 内部の **i18n 整備リリース**。CLI ヘルプ・エラーメッセージ・viewer UI 文字列を `rust-i18n` で動的解決に切り替え、同時に Rust ソース内に残っていた日本語コメントと `assert!` / `expect()` / `panic!` メッセージをすべて英語化した。動作上は UI ラベルが従来どおり日本語で表示されるため、エンドユーザー視点の挙動は v0.5.8 から不変。あわせて右側パネルのリサイズ可能化・UV 編集ウィンドウのリサイズ挙動改善・`log_viewer.toml` のフォーマット統一など UI 周りの改善を同梱する。

### 新機能 / 改善

- **右側パネルのリサイズ可能化＋幅永続化** — トップバー右側のタブパネル（情報 / 表示 / 出力 / アニメ / アーカイブ等）を `egui::SidePanel::resizable(true)` でリサイズ可能化。境界をマウスドラッグで幅を変更でき、変更後の幅は `popone.toml` の `[window] right_panel_width` で永続化される。これまで右側ペインは固定幅で、材質編集パネルやファイルツリーの可視範囲が窮屈になっていた問題を解消
- **材質編集パネル表示時のモデル表示倍率維持** — 右側パネル幅の変更に伴って 3D ビューポート幅も変わるため、材質編集パネルを開閉するとモデルの見た目サイズが変動していた。パネル開閉時にカメラ距離を補正することで、視野角を変えずにモデルの見た目サイズを保つよう修正
- **UV 編集ウィンドウのリサイズ挙動改善＋ UV 表示 1:1 アスペクト固定** — UV 編集ウィンドウを `egui::Window` のリサイズ可能化＋最小サイズ指定で大小自由に表示できるよう改善。キャンバス内では UV 空間を **1:1 アスペクト**で固定描画し、ウィンドウを縦長／横長にリサイズしても UV の縦横比が崩れない
- **`log_viewer.toml` のフォーマットを `popone.toml` と統一** — ログビュアーウィンドウの位置・サイズ永続化形式を、本体側 `popone.toml` と同じ `[window]` セクション形式（`outer_x` / `outer_y` / `inner_w` / `inner_h`）に揃えた。これにより両 toml の構造が同一となり、外部ツールや手動編集での読み書きが一貫する

### 内部実装（i18n 整備）

このリリースの主目的は **`rust-i18n` を用いた CLI / viewer 文字列の動的解決化＋ Rust ソースの英語化**で、`v0.5.8..v0.5.9` 区間の約 50 コミットに分かれる。プロジェクトポリシーは **ログ＝英語固定 / UI＝ロケール切替可能（`t!()` 経由）/ ソースコメント＝英語**。

- **CLI / 内部エラーの i18n** — `89a00e0`〜`bb8d7e3` で CLI ヘルプ、`--dump` 出力、エラーメッセージ、`Error:` プレフィックス、`anyhow::Context` チェーン内の日本語文字列、`thiserror` 派生の `#[error]` 属性、loaders / archive / vrm/extract / pmx/build / unitypackage / obj/directx の各ローダーを横断的に i18n 化（`t!()` 経由）。テクスチャ展開や材質ロードなどローカライズ不可能だった深部の日本語埋め込みを撤廃
- **viewer UI 文字列リテラル → `t!()` 化** — A-1 から A-9 までの段階で viewer 全 UI を i18n 化。順序: side-panel skeleton（タブ＋セクション見出し）→ top / status / shortcut bars → 6 dialogs（33 キー）→ toasts（cancel / precondition / bg_failure / progress / append / anim / reload / uvmap / texture / history）→ material editor + texture drop dialog → UV edit window → animation controls → VRM meta panel（permissions / license 辞書）→ display tab + morph filter（A-1）→ info tab + texture picker + PMX badge（A-2）→ material list / texture column（A-4）→ export tab + convert toast + uv_edit hints（A-5）→ log viewer window（A-6）→ status bar + D&D overlay（A-7）→ ImportUnit + progress overlay + cancel（A-8）→ file tree + MMD texture section（A-9）→ leftover cleanup（PMX log + IPC eprintln）。各バッチ後に `cargo clippy --features viewer -- -D warnings` でリンタ警告ゼロを維持
- **viewer の `assert!` / `expect()` / `panic!` メッセージ英文化** — パニック時に翻訳ロードが保証されないため i18n 対象外として全て英語に統一。初期 11 件 + 追加 40 件 = 計 51 箇所
- **viewer ソースコメントの英文化** — viewer/ 配下のコメントを英語化（small files batch 1〜5 + large files batch 1 + 最大ファイル `app/file_io.rs` + `app/mod.rs` + `gpu.rs` + `ui.rs`）。viewer 配下の日本語コメント残数は **3,646 → 0 件**で完全消滅
- **non-UI ソースコメントの英文化** — convert 系 / 中規模ファイル / archive / unity / ray-mmd MME / 中間データ型 / pmx/build / vrm/extract / テストコメント等の非 UI 部のコメントを batch 2〜4f で英語化
- **uvmap テストの追従修正** — 内部エラーメッセージの i18n 化に伴って 2 件のユニットテスト（`uvmap` 関連）が固定文字列マッチで失敗していたのを、`t!()` 解決後の英語メッセージに同期させて復旧

### スコープ注記

- ユーザー画面に表示される日本語 UI ラベル群は `t!()` で動的解決される構造になっただけで、表示文言・レイアウトは v0.5.8 から変わっていない。`popone.toml` への永続化スキーマも `right_panel_width` の追加以外は互換
- ログ言語は **英語固定**（`log` クレート出力先）。UI ロケールは将来 `popone.toml` 経由で切替可能にするための足場が整った状態で、本リリース時点ではデフォルトロケール（日本語）のみが提供される

## v0.5.8（2026-04-22）

CI 環境の Rust ツールチェインをリポジトリ側で固定し、ローカルと GitHub Actions のビルド再現性を揃えるためのメンテナンスリリース。動作・機能上の変更はなし。

### 内部実装

- **`rust-toolchain.toml` を新規追加** — `popone/rust-toolchain.toml` に `channel = "1.93.1"` / `components = ["rustfmt", "clippy"]` / `profile = "minimal"` を宣言。`popone/` 配下で `cargo` を実行すると `rustup` が当該バージョンを自動でインストール・選択するため、開発者環境のばらつき（stable のマイナー差分など）が原因のローカル限定エラーを排除できる
- **`Cargo.toml` に `rust-version = "1.93"` を追加** — Cargo メタデータ上の MSRV を明示。古い toolchain で `cargo install` / `cargo build` した際に、コンパイル開始前に分かりやすいエラーメッセージで弾かれるようになる
- **CI ワークフロー (`popone/.github/workflows/ci.yml`) の Rust 取得を `dtolnay/rust-toolchain@stable` から `actions-rust-lang/setup-rust-toolchain@v1` へ切り替え** — `dtolnay/rust-toolchain` は `rust-toolchain.toml` を自動読み取りせず、`toolchain` input が必須となるため、ローカル側 `rust-toolchain.toml` との二重管理を強いられる。`actions-rust-lang/setup-rust-toolchain@v1` は Rust workgroup が公式にメンテしているアクションで、デフォルトで `rust-toolchain.toml` を読み取り、`channel` / `components` / `profile` をすべて尊重するため、CI ファイル側に Rust バージョンや components の重複指定が一切不要になる
- **CI キャッシュキーに `rust-toolchain.toml` を追加** — `actions/cache@v4` の key を `hashFiles('Cargo.lock')` から `hashFiles('rust-toolchain.toml', 'Cargo.lock')` に拡張。将来 Rust バージョンを更新したタイミングで `target/` キャッシュが自動失効するため、コンパイラ差分由来の古いオブジェクト混入事故を予防する

## v0.5.7（2026-04-22）

PMX モデルが参照するテクスチャパスがディスク上に実在しない場合に、マゼンタ 1×1 のフォールバックが使われて顔などにピンク色の色被りが出る問題に対応。フォールバック色をランタイムで切り替えられる表示オプションも追加。

### 新機能

- **テクスチャ欠落時の白フォールバック（デフォルト）** — PMX の内部テクスチャリストが実体のないパスを指している場合（例: `textures\Skin.png` を参照しているが実物は `toon\` 配下にしかない等）や、画像デコードに失敗した場合、従来は **1×1 マゼンタ** の画像を GPU に焼いて当該材質に使っていた。toon/sphere のように乗算・加算合成で参照されるスロットでこれを使うと、顔などの材質全体に強いピンク/マゼンタの色被りが発生する。v0.5.7 からフォールバックを **1×1 白 (255,255,255,255)** に変更し、他のライティング経路を一切変えずに色被りを解消する
- **表示オプション: `テクスチャ欠落時フォールバックを白に` トグル** — 表示タブの MSAA 項目の下に、白（既定）と従来のマゼンタを切り替えるチェックボックスを追加。マゼンタは「欠けているアセットを目立たせたい」診断用途のために残してある。`popone.toml` の `[display] white_texture_fallback` として設定は永続化される
- **動的切替** — トグルは即時反映。モデル再読込は不要で、全失敗経路が 1 枚の共有 1×1 テクスチャを参照するよう統一されているため、切替時は `queue.write_texture` で 1 ピクセル (4 バイト) を書き換えるだけ。材質の BindGroup や描画パイプラインには一切触らず、次のフレームから新しい色で描画される

### 内部実装

- `viewer/texture.rs` に `SharedFallback { tex, srgb_view, unorm_view }` シングルトンを `Mutex<Option<_>>` で管理。最初の失敗経路アップロード時に遅延初期化する。3 つのフォールバック経路 — `IrTexture.data` 空（`upload_single_texture`）、`decode_image_to_rgba_with_hint` 失敗（同）、非対応 `gltf::image::Format` 分岐（`upload_textures`）— の全てで、共有 sRGB / Unorm `TextureView` のクローンを返す。wgpu の `TextureView::clone` は内部 Arc 参照カウントを増やすだけなので、失敗発生ごとに 1×1 `wgpu::Texture` を作っていた従来から GPU アロケーション 0 に改善
- `set_white_texture_fallback_dynamic(enabled, &queue)` で `AtomicBool` を更新し、共有テクスチャが初期化済みなら `queue.write_texture` で 4 バイトを書き込む。GPU の View 参照は不変のため BindGroup 再構築は不要で、描画途中にトグルしても安全
- `DisplaySettings` に `white_texture_fallback: bool` フィールド（既定 `true`）を追加。永続化のために `AppConfig.display: DisplayConfig` セクションを新設。`DisplayConfig` は全フィールドに `#[serde(default)]` を付与しており、`[display]` ブロックのない旧 `popone.toml` も問題なく読み込める

## v0.5.6（2026-04-14）

UV エディタの後続改善 2 件を実装。

### 新機能

- **PMX UV モーフの IR→PMX ラウンドトリップ書き戻し** — v0.5.5 まで `IrMorphKind::Uv` は PMX writer で空 Group としてスタブ化され、UV モーフ編集後に PMX 保存しても情報が失われていた。`build.rs` の `build_morphs` で IR グローバル頂点 index → PMX 頂点 index の対応（`build_vertices_and_faces` の mesh.vertices 順次 push と恒等）を活用し、`PmxMorphOffsets::Uv` と morph_type（UV0=3, UV1..4=4..7）を直接書き出すよう変更。これにより「PMX 読込 → UV モーフ編集 → PMX 保存 → 再読込」のラウンドトリップが成立する。同一頂点の重複オフセットは合算し、`vertex_index` でソートして出力の決定性を保証
- **モーフ編集中のウェイト自動 1.0 化** — UV エディタで UV モーフを編集モードに切り替えた瞬間、`app.morph_weights[active_morph]` を退避して自動的に `1.0` にセット。編集モード終了時（`None` への切替やリスト外フォールバック）に元値で復元する。退避/復元は `UvEditState::switch_active_morph` ヘルパーに集約されるため、ComboBox 切替・IR 変更時のリスト外チェック・モーフ削除等のあらゆる経路で一貫した振る舞いになる
- **モーフ編集中のサイドパネルスライダーロック** — 編集対象モーフは「表情モーフ」サイドパネル上でスライダー・`0`/`1` ボタン・DragValue がすべて無効化され、ラベル横に `(UV編集中)` の補助表示を追加。「全リセット」ボタンも UV 編集中モーフをスキップ対象にし、退避値とのズレを防ぐ

### 内部実装

- `UvEditState` に `morph_weight_saved: Option<f32>` フィールドを追加。`switch_active_morph(new_morph, &mut weights)` ヘルパーを新設し、active_morph の書き換えはこのヘルパー経由に統一（直接代入を禁止する設計）。`reset()` でも `morph_weight_saved = None` にして、リロード時の古い IR index を確実に破棄する
- `pmx/build.rs` の `build_morphs` ログ統計に `uv` カウントを追加（`Morphs: N (vertex=A, group=B, uv=C)`）。境界外 index は警告ログ＋スキップで防御

### バグ修正（リリース前レビュー対応）

- **[Codex 0.5.6/01 P1]** UV モーフ編集中に reload や A/T スタンス変換を行うと、`save_reload_snapshot` が「1.0 に固定された一時ウェイト」を退避してしまい、`finish_load_with_gpu` で `uv_edit.reset()` により `morph_weight_saved` が破棄された結果、reload 後に対象モーフのウェイトが 1.0 のまま恒久化していた。`save_reload_snapshot` 冒頭で `switch_active_morph(None, &mut self.morph_weights)` を呼んで snapshot 直前に元ウェイトへ復元するよう修正。snapshot には常にユーザー意図のウェイトが入る
- **[Codex 0.5.6/02 P1]** UV モーフ編集中の `overrides` は「base UV + morph offset」の表示値を持つが、キーに morph 区別情報がないため、reload 後の `apply_to_ir` がそのまま base UV へ焼き込んでいた。再度 morph を有効にすると offset が二重適用されて見た目と内部データ両方が壊れる。`save_reload_snapshot` で morph 編集中だった場合に `overrides` / `pristine_uvs` / `undo` / `redo` / `selected` を一括クリアするよう修正。morph 編集結果は `write_displayed_uv` 経由で IR に直接反映済みのため、reload 越しに overrides を保持する必要がない（`overrides` の役割を「base UV 編集の永続化専用」に厳格化）
- **[Codex 0.5.6/03 P1]** 0.5.6/02 の修正で `overrides` を捨てた代償として、未保存の UV モーフ編集が reload で黙って失われていた（`write_displayed_uv` が旧 IR に書いた offsets は新 IR 構築時に失われる）。`ReloadSnapshot` に `uv_morph_offsets` を追加して旧 IR の全 UV モーフ offsets を退避し、`restore_snapshot_on_success` で新 IR の同名モーフに書き戻すよう修正。channel 不一致の場合は警告ログ＋スキップ。編集していないモーフは同値上書きで no-op
- **[Codex 0.5.6/04 P1]** 0.5.6/03 で導入した `uv_morph_offsets` は `HashMap<name, ...>` だったため、同名の UV morph が複数あると `.collect()` で後勝ち上書きされ片方の編集内容が失われていた（VRM/glTF で `name_en` 空の同名衝突は実在する）。`UvMorphOffsetEntry { name, name_en, channel, offsets }` の `Vec` に変更し、復元時は **未使用フラグ + `(name, name_en, channel)` の完全一致** で一意マッチングするよう修正。同名 N 個があっても N 番目に正しく復元される。一致しない snapshot エントリは警告ログ付きで破棄

## v0.5.5（2026-04-13）

**材質編集パネルから呼び出す頂点単位 UV 編集ウィンドウ**を追加。v0.5.4 は材質単位の UV 変形（offset / scale / rotation）を提供したが、v0.5.5 はその下のレイヤーに踏み込み、**Phase 1**（単一頂点エディタ＋永続化＋reload 保持）、**Phase 2**（テクスチャ背景・矩形選択・ズーム/パン・回転/スケール・undo/redo・Ctrl+A）に加え、**Phase 3** の全項目（矩形選択の加算/除外、独立 OS ウィンドウ化、UV1 編集、2D ギズモハンドル、PMX UV モーフ編集）を同梱する。

### 新機能 (Phase 1)

- **UV 編集ウィンドウ** — 材質編集パネルのヘッダに「UV 編集」ボタンを追加。クリックでフローティング `egui::Window`（`Id::new("uv_edit_window")` で 1 インスタンス固定）が開き、タイトルにアクティブ材質名を表示。ウィンドウ内に最大 260×260 px の正方形キャンバスでアクティブ材質の三角形ワイヤーフレームを UV 空間（**v=0 を上端 / v=1 を下端**）で表示。`convert/uvmap.rs` の PSD 出力と同じ向きなので両者を直接並べて比較できる
- **頂点ピック & ドラッグ** — UV 頂点から 12 px 以内をクリックすると選択（黄色）、ドラッグで UV 空間上を平行移動。編集結果は `IrMesh.vertices_mut()[*].uv` に直接書き込まれるため、再エクスポート（PMX 出力）に即時反映される
- **材質フィルタ** — ウィンドウ内 ComboBox でキャンバスに表示する材質を切り替え。材質編集パネルの「UV 編集」ボタンからの起動時は現在編集中の材質を自動でアクティブにセット
- **頂点 UV の永続化** — `TextureHistoryFile` に `vertex_uv_overrides: HashMap<path, Vec<VertexUvOverrideEntry>>` を追加（JSON バージョンを v3 に昇格）。「履歴を保存」で頂点単位 UV 差分をテクスチャ・パラメータ差分と並んで書き出し、「履歴呼出」で復元して GPU vertex buffer を再同期
- **mouse-up 時のみ GPU 同期** — `GpuModel::sync_uvs_from_ir` はドラッグ終了時にのみ vertex buffer を全転送。ドラッグ中は CPU 側の `IrVertex.uv` 更新のみでフレームレートに影響しない

### 新機能 (Phase 2)

- **テクスチャ背景 (2-1)** — アクティブ材質の BaseColor テクスチャを `register_native_texture` + `painter.image` でキャンバス背景に描画。UV [0,1] 領域のみを対象とする。1 エントリキャッシュ（`ViewerApp.uv_edit_bg_tex`）で材質変更時のみ張り替え、`finish_load_with_gpu` でモデル切替時に `free_texture` して GPU リークを回避。PMX/PMD 材質で `base_color_tex_info` が `None` の場合は `mat.texture_index` にフォールバック
- **矩形選択 + 一括平行移動 (2-2)** — 頂点から遠い位置でドラッグ開始すると矩形選択（既存選択をクリア、矩形内頂点を毎フレーム再計算して selected に反映）。頂点近傍から開始すると Move モードに入り、選択頂点全体を平行移動。`UvDragMode { None, Move, Rect }` でモード分岐、`drag_start_uvs` は HashMap なので 1 頂点でも N 頂点でも同一コードパス
- **ズーム / パン / スナップ (2-3)** — ホイールでカーソル中心にズーム（0.1×〜32×、対数スケール `* 0.002.exp()`）。中ボタンドラッグでパン（ズーム倍率を考慮）。Shift+ドラッグで平行移動を 1/16（= 0.0625）グリッドにスナップ。`uv_to_canvas` / `canvas_to_uv` に `view_offset` / `view_zoom` を引数追加し、ピック/描画/ドラッグ/矩形計算すべてが自動でビュー変換に追従。「表示リセット」ボタンで zoom=1.0 / offset=[0,0] に復帰
- **回転 / スケール (2-4)** — Alt+ドラッグで選択 bbox 中心ピボットの回転（`atan2` で角度差 → `sin_cos` で回転行列）。Ctrl+ドラッグで同ピボットのスケール（距離比）。Move ドラッグ中はピボットに十字マーカーを描画して視覚フィードバック。Ctrl と Alt が同時押下の場合は Ctrl 優先
- **undo / redo (2-5)** — `UvUndoEntry { before, after }` を drag_stopped Move ごとに 1 コマンド記録。`Ctrl+Z` で undo、`Ctrl+Y` / `Ctrl+Shift+Z` で redo。「⟲ 元に戻す」「⟳ やり直す」ボタンが GUI から同等操作を提供。undo スタックは 50 エントリ上限（FIFO）。新編集で redo スタックは自動クリア（標準セマンティクス）。`wants_keyboard_input()` ガードで TextEdit とキー衝突しない
- **全選択 (Ctrl+A)** — アクティブ材質の全頂点を `selected` に追加（既存選択は保持）。「全選択」ボタンが GUI から同等操作を提供

### 新機能 (Phase 3)

- **矩形選択の加算/除外モード (A-4)** — 従来の矩形選択は常に既存選択を置換していたが、Shift+ドラッグで矩形内頂点を既存選択に追加、Ctrl+ドラッグで除外する動作を追加。`UvRectBehavior { Replace, Add, Subtract }` を drag_started 時の修飾キーで確定し、`rect_initial_selected` に開始時点の selected をスナップショット保存。矩形拡大/縮小時の整合は毎フレーム `initial ± inside` で再計算する方式で保つ。Alt は Move モードの「回転」と競合するため、Rect モードの除外は Ctrl を使う（mode は drag 開始位置で確定しているので Ctrl の意味は mode ごとに一意）
- **独立 OS ウィンドウ化 (A-3)** — UV 編集ツールバーに「⬈ 分離」ボタンを追加。押下で `egui::Window` によるメインウィンドウ内フローティングから `ctx.show_viewport_immediate`（eframe 0.31 の viewport API）による OS ネイティブの独立ウィンドウへ切り替え、独自のタイトルバー・リサイズ・最小化/閉じるボタンを持つ別デスクトップウィンドウに UV エディタを分離する。メインビューアは背後で 3D シーンを描画し続ける。「⬓ 結合」でメインウィンドウ内に戻す。`UvEditState.detached: bool` がセッション中の設定として値を保持し（reset では変更しない）、`ViewportId::from_hash_of("uv_edit_viewport")` で OS 側のウィンドウ位置/サイズをトグル間で維持。独立ウィンドウの × ボタン押下時は `uv_edit_window_open = false` に戻し、次回 UV 編集を開いた際は独立状態のまま開く。
- **UV1 編集 (A-1)** — `VertexKey` を `(mesh_idx, vertex_idx)` から `(mesh_idx, vertex_idx, uv_set)` に拡張し、`uv_set = 0` は `IrVertex.uv` (UV0)、`uv_set = 1` は `IrMesh.uvs1[vi]` (UV1 / `TEXCOORD_1`) を指す。新設の「UV セット」ComboBox で UV0 / UV1 を切替可能（アクティブ材質に属するメッシュが UV1 を 1 つも持たない場合は UV1 オプションを自動で disable する）。UV セット切替時は進行中ドラッグを取り消す一方、`selected` / `overrides` / undo 履歴はチャネルごとに別空間として保持されるため UV0/UV1 が混線することはない。ピック/描画/ドラッグ/矩形選択/Ctrl+A の全パスが `active_uv_set` でフィルタされ、UV1 選択時は UV1 を持たないメッシュをスキップし、書き込みは新設の `write_vertex_uv(ir, mi, vi, uv, chan)` 経由で振り分ける。`sync_uvs_from_ir` は UV0 / UV1 両方を GPU vertex buffer（`animated_vertices` 含む）へアップロードするため、UV1 の編集結果は MToon の UV1 ルックアップ・Matcap 等にも drag-stop 時点で反映される。`VertexUvOverrideEntry` に `uv_set: u8` を `#[serde(default)]` 付きで追加したので、v0.5.5 Phase 1 で書き出した UV0 のみの履歴ファイルはそのまま読み込める。
- **視覚 2D ギズモハンドル (A-5)** — 選択頂点が 2 個以上かつ面積 0 でない bbox を持つとき、選択 bbox を橙色の枠線で描き、4 隅にスケールハンドル（橙色の四角）、上辺外側 24 px に回転ハンドル（青色の丸）を配置する。角ハンドルをドラッグすると「掴んだ角の反対角」を pivot としてスケール（Photoshop/Blender 流）、回転ハンドルは bbox 中心を pivot として回転する。ギズモ経由のドラッグは修飾キーを必要としない — `UvGizmoAction { ScaleCorner { sign_u, sign_v }, Rotate }` が `drag_started()` 時点のヒットテストで確定し、Move ブランチでは修飾キー解釈より優先される。従来の修飾キー方式（Ctrl=スケール、Alt=回転）も互換のために維持される。ハンドルのピック半径は 10 px。回転ハンドルと角の pick 領域が重なった場合は回転を優先する。
- **PMX UV モーフ編集 (A-2)** — これまで破棄されていた PMX モーフタイプ 3〜7（UV0 モーフ / 追加 UV1〜UV4 モーフ）を IR に `IrMorphKind::Uv { channel, offsets: Vec<(global_vi, [f32; 4])> }` として取り込むようにした。GPU モーフパイプラインに `GpuMorphEntry::Uv` バリアントを追加し、`apply_gpu_morph_recursive` がモーフ適用毎に `(du, dv) * weight` を `vertex.uv` (channel=0) または `vertex.uv1` (channel=1) に加算合成する。UV エディタに「編集対象」ComboBox を追加し、モデルが持つ `Uv` モーフを列挙、選択でキャンバスが「モーフ編集モード」に切替わる。このモードでは read/draw/pick/drag/gizmo が「ベース UV + モーフオフセット」で動き、書き込みは `write_displayed_uv` / `read_displayed_uv` 経由でそのモーフの頂点別オフセットマップを更新する。モーフ選択時は `active_uv_set` をモーフの channel に強制同期し、進行中のドラッグ状態・選択・undo 履歴をクリアすることで UV0 / UV1 / 各モーフの編集空間を独立に保つ。**制約:** (1) channel >= 2 (UV2〜UV4) は読み込み・保持のみで GPU 合成対象外（頂点シェーダーが UV0/UV1 しか持たないため）、(2) IR→PMX writer は `IrMorphKind::Uv` を現状空 Group として書き出す（PMX 頂点 index への逆マップ未保持のため、ラウンドトリップ対応は将来バージョンで追加）、(3) UV エディタのプレビューは weight=1.0 を前提としているため、3D ビューポートで反映を確認するには side panel でモーフウェイトスライダーを 1.0 まで動かす必要がある。

### 内部実装

- 新規 `src/viewer/app/uv_edit.rs` に `UvEditState` を実装し、フェーズを追うごとに拡張:
  - Phase 1: `overrides`, `selected`, `active_material`, `dragging`, `pending_restore`
  - Phase 2-2: `UvDragMode` enum, `drag_mode`, `drag_start_uvs`, `drag_press_uv`
  - Phase 2-3: `view_offset`, `view_zoom` (`view_zoom = 1.0` のため `Default` を手書き), `reset_view()`
  - Phase 2-4: `drag_pivot`
  - Phase 2-5: `UvUndoEntry`, `undo_stack`, `redo_stack`, `push_undo`, `apply_undo`, `apply_redo`, 定数 `UV_UNDO_MAX = 50`
  - レビュー 05 対応: `pristine_uvs`, `record_pristine`, `matches_pristine`（遅延記録の per-vertex pristine。undo が pristine に戻した頂点を overrides から自動除外）
- `ViewerApp` に `uv_edit_window_open: bool` と `uv_edit_bg_tex: Option<(usize, egui::TextureId)>` を追加。`show_uv_edit_window` は `update()` の `show_material_editor_window` 直後に呼び出し、両フィールドとも `finish_load_with_gpu` でクリア / free される
- `persistence.rs` に `VertexUvOverrideEntry { mesh_index, vertex_index, uv: [f32; 2] }` を追加（フラット配列 JSON ~30 byte/頂点）。`TextureHistoryFile` は `#[serde(default)]` により v0.5.4 以前の `popone_history.json` もそのまま読める後方互換
- `GpuModel::sync_uvs_from_ir` は IR メッシュを走査し、既存の `global_to_gpu` マップで GPU 頂点Index を逆引きして `base_vertices[*].uv` を書き換え、`queue.write_buffer` で転送後にモーフキャッシュを無効化する
- `uv_to_canvas` / `canvas_to_uv` に `view_offset: [f32; 2]` / `view_zoom: f32` 引数を追加。描画・ピック・ドラッグ・矩形計算がすべてこの 2 関数経由なので、Phase 2-3 のパン/ズームが 1 箇所の変更で全インタラクションに波及
- ドラッグ処理は `start_uv + (cursor_uv - press_uv)` 方式（および回転/スケール の類似式）で egui バージョン間の `drag_delta()` 仕様に依存しない

### スコープ注記

- **UV0 のみ対応**。UV1（`IrMesh.uvs1`）・モーフ UV・複数 UV セット切替は Phase 3（v0.5.7 以降）で対応予定
- **単一ウィンドウインスタンス**。複数材質の横並び編集は Phase 3
- **「編集をすべてクリア」は破壊的**。overrides / selected / undo スタック / redo スタック / pristine_uvs を全て破棄する。編集前の UV に完全に戻すにはリロードが必要（モデル全体の pristine を保持しないため）

### バグ修正（リリース前レビュー対応）

- **review_result_01 [P1] reload で頂点 UV 編集が消える問題を修正** — `finish_load_with_gpu` が無条件で `self.uv_edit.reset()` を呼んでいたため、A スタンス / T スタンス変換や `reload_current()` を挟むと未保存の UV 編集が失われていた。`ReloadSnapshot` に `uv_edit_overrides` / `uv_edit_active_material` / `uv_edit_window_open` を退避し、`restore_snapshot_on_success` で `apply_to_ir` による IR 反映と `sync_uvs_from_ir` による GPU vertex buffer 再同期を行う。`restore_snapshot_on_failure` でも overrides を書き戻し、失敗時のメモリ状態の整合を保つ
- **review_result_02 [P1] ドラッグ移動が毎フレーム過加算される問題を修正** — 旧実装は `dragged()` フレーム毎に `response.drag_delta()` を現在 UV へ加算していたが、egui バージョンによっては累積量を返すケースがあり、フレーム数に比例して UV が飛んでいた。新実装は `drag_started()` で開始 UV とカーソル UV を記録し、`new_uv = start_uv + (cursor_uv - press_uv)` で再計算する方式に変更。`canvas_to_uv` ヘルパーと `UvEditState::{drag_start_uvs, drag_press_uv}` を追加
- **実機確認 [P1] UV キャンバスと PSD 出力の Y 方向が逆転** — 初期 Phase 1 実装は Blender/Maya 慣例で v=0 を下端に配置していたが、`convert/uvmap.rs` は UV v をそのまま画像 Y にラスタライズ（v=0 が上端）していた。`uv_to_canvas` / `canvas_to_uv` を両方 Y 非反転に変更し、PSD 出力と同じ向きに統一
- **review_result_03 [P2]「編集をすべてクリア」が undo/redo 履歴を残していた** — 旧実装は `overrides` / `selected` しかクリアせず、直後に `Ctrl+Z` を押すと破棄したはずの UV 編集が復活し、UI ラベルの契約に反していた。`undo_stack` / `redo_stack` も同時破棄し、ホバーテキストに「undo/redo 履歴を破棄」を明記
- **review_result_05 [P2] undo 後も元 UV が override として残っていた** — `apply_undo` / `apply_redo` が常に `overrides.insert(k, v)` していたため、頂点を元位置に戻しても overrides にエントリが残り、UI 件数と `to_entries()` 経由の保存結果が実状態とずれていた。遅延記録の `pristine_uvs`（drag_started で `or_insert`）を導入し、適用後の UV が pristine と一致するなら `overrides.remove` するように変更。メモリは「一度でも編集された頂点」分のみで、Phase 1 で避けた全頂点 pristine とは別物の軽量設計
- **review_result_06 [P2]「編集をすべてクリア」が pristine_uvs を残していた** — レビュー 05 対応後、クリアボタンは `pristine_uvs` も破棄する必要が判明。残したままだと次の編集セッションで古い pristine を `or_insert` で再利用し、undo で「未編集状態」に戻れなくなる不具合があった

### テスト

- 既存 179 テスト全通過。UV 編集ロジックは UI 主体で、下流のデータフロー（IR → GpuModel → PMX writer）は既存のラウンドトリップテストで既にカバー済み

## v0.5.4（2026-04-13）

材質編集パネルにスロット毎の UV 変形（offset / scale / rotation）編集 UI を追加。KHR_texture_transform を持つ 9 スロット（BaseColor / Emissive / Normal / Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask）が対象。

### 新機能

- **スロット毎 UV 変形編集 UI** — 各テクスチャスロットのサムネイル直下に UV 変形コントロール（offset X/Y、scale X/Y、rotation°、リセットボタン ⟲）を追加。テクスチャが割り当てられているスロットにのみ表示され、値は `IrTextureInfo.offset / scale / rotation` に即時反映されて GPU uniform（`base_uv / shade_uv / ...` の 9 ペア）へ送信される。rotation は度入力・ラジアン保存
- **UV 変形の永続化** — `MaterialParamOverride` に 9 スロット分の `TextureUvOverride { offset, scale, rotation }` フィールドを追加。`popone_history.json` にモデルパス単位で保存され、reload / A スタンス変換 / 再起動後に自動復元される。v0.5.3 以前の JSON は `#[serde(default)]` によりフィールド欠落でそのまま読込可能
- **Expression 駆動 UV アニメとの共存** — v0.5.1 で配線済の `IrTextureTransformBind` とは独立経路で動作し、静的 UV override → Expression 加算の順で両立

### 内部実装

- `TextureUvOverride` 型を新規追加（`offset: Option<[f32;2]>`, `scale: Option<[f32;2]>`, `rotation: Option<f32>`）。全フィールド `Option` のため部分保存が可能で、serde でのサイズも最小化
- `apply_to` は **既存 `IrTextureInfo` が Some の場合のみ** UV を書き込む。未割当スロットに対しては no-op（勝手に `IrTextureInfo::from_index(0)` を生成して誤テクスチャを差し込まない）
- MToon スロット UV は `mat.mtoon.as_mut()` 経由で書き込み、`mat.mtoon_mut()` は呼ばない（非 MToon 材質に default mtoon を自動挿入しないための防御）
- `diff_from` は `enable_mtoon == Some(false)` の場合、MToon スロット UV 6 種も含めて diff を skip（round-trip 一貫性）
- UI ヘルパー `uv_transform_widget` / `record_uv_override` を `ui.rs` に追加し 9 箇所で共用

### バグ修正（リリース前レビュー対応）

- **review_result_01 [P1] 新規割当スロットの UV 編集が履歴保存されない** — `TextureUvOverride::diff()` が `(Some, Some)` の場合しか差分を返さず、pristine が未割当で current が新規割当 + UV 編集済みのケースで履歴保存時に UV 差分が落ちていた。`(None, Some)` のときも default transform（offset=0 / scale=1 / rotation=0）との比較にフォールバックし、新規割当スロットの UV 編集を正しく保存するよう修正

### テスト

- UV round-trip: BaseColor / MToon スロット (shade) の diff → apply が等価（+2 件）
- 未割当スロットへの apply が no-op であること / mtoon 未初期化時に mtoon を生成しないこと（+2 件）
- MToon OFF 時に MToon スロット UV が diff に含まれないこと（+1 件）
- `TextureUvOverride::default()` の `is_empty` 動作 / `merge_from` の UV マージ（+2 件）
- 新規割当スロット UV の保存 / スロット解除時の UV 扱い（+2 件、review_result_01 [P1] 対応）
- 合計 +9 件（material_edit tests 計 19 件、全体 244 件パス）

## v0.5.3（2026-04-13）

材質編集 UI の刷新: フローティング Window からショートカットヒントバー直上の固定ドック型パネルへ変更。材質名編集、先頭ボタンのサムネイル表示、アイコン絵文字化、法線一括操作の ON/OFF ボタン化を一括適用。

### 新機能

- **材質名編集** — 材質編集パネル冒頭に TextEdit を追加し、材質名をその場で変更可能。`MaterialParamOverride.name: Option<String>` 経由で `material_overrides` に記録され、reload / A スタンス変換 / `popone_history.json` 保存すべてで復元される。変更後は `update_mat_cache()` でサイドパネルの材質リスト表示に即時反映
- **材質編集パネルの固定ドック化** — 旧フローティング `egui::Window` を `egui::TopBottomPanel::bottom("material_editor_panel")` に変更。ショートカットヒントバー直上に固定表示し、上辺ドラッグで伸縮、内部は `ScrollArea::vertical` でスクロール可能。右上の `[×]` ボタンで閉じる。[編] アイコンが OFF のときはパネル自体が出現せず、中央ビューポートが自動拡張する
- **材質行先頭ボタンのサムネイル化** — 旧 □/■ 文字インジケータを `ir_thumb_cache` の 14×14px テクスチャサムネ `ImageButton` に置換。`ui.scope` 内でのみ `spacing.button_padding = (1,1)` + `stroke = 0.5` を適用するコンパクト枠プリセットで、他 UI ボタンに影響を与えずに絵文字アイコン列と視覚的に揃える。サムネ取得不可時は従来の □/■ にフォールバック（割当済だがサムネ未生成時は ■）
- **アイコン絵文字化** — 材質行と材質グループヘッダの `[S][C][N][B][編]` ラベルを `✨🗑🗺💡✏` に置換。`ICON_SMOOTH / ICON_CLEAR_NORMAL / ICON_NORMAL_MAP / ICON_EMISSIVE / ICON_EDIT` を `ui.rs` 冒頭の定数として 1 箇所に集約
- **法線一括操作の ON/OFF ボタン化** — 旧「法線平滑化（一括）」「カスタム法線クリア（一括）」チェックボックスを `ラベル + [on] + [off]` の小ボタン列に置換。on 操作時の法線マップ付き材質スキップ仕様は維持

### 内部実装

- `MaterialParamOverride` に `name: Option<String>` フィールドを追加。`String` は `Copy` でないため `merge_from` / `diff_from` / `apply_to` では既存の `merge!` / `diff_field!` マクロ対象外として個別 `clone()` で処理
- `update_mat_cache` の可視性を `pub(super)` → `pub(in crate::viewer)` に拡張し、`ui.rs` から材質名キャッシュを再構築できるようにした
- `show_side_panel` 先頭で `app.sync_ir_thumb_cache()` を呼ぶよう変更（length 比較で早期 return するため同期済なら無コスト）
- 材質編集パネルの呼び出し位置を `apply_pending_material_rebuilds()` 前から `shortcut_hints` パネル追加の直後に移動し、egui の下部パネル積み上げ順「最下=status_bar / 中=shortcut_hints / 上=編集パネル」を確保

## v0.5.2（2026-04-13）

材質編集ドロワーの各パラメタセクションにテクスチャサムネイルを統合。テクスチャと関連パラメタが 1 箇所で見られるようになった。

### 新機能

- **テクスチャサムネイルのセクション統合** — 旧「テクスチャスロット」集約セクションを解体し、各材質セクションの先頭にサムネイル + 割当UI を配置した:
  - **基本**: BaseColor
  - **影 (Shade)**: Shade / ShadingShift
  - **アウトライン**: OutlineWidth
  - **リム**: RimMultiply
  - **MatCap**: Matcap
  - **UV アニメ**: UvAnimMask
  - **エミッシブ / 法線**: Emissive / Normal
  - **MMD テクスチャ (Sphere / Toon)**: Sphere / Toon（MMD/PMX 固有のため別セクションとして残置）
- **サムネイルがそのままボタン** — 32px のサムネイル画像自体が `ImageButton` になり、クリックでファイルダイアログが開く（従来のテキストボタンは廃止）。ホバーでファイル名をツールチップ表示。
- **未割当スロットは X アイコン** — 割当のないスロットには `×` 印のプレースホルダボタンを描画。色はテーマの `widgets.inactive` に連動。クリックで新規割当ダイアログが開く。
- **行末の `×` はスロットリセット** — 既存挙動を踏襲し、割当済みスロットのみに表示（小さな `×` ボタン）。

### 内部実装

- `TextureState` に `ir_thumb_cache: Vec<Option<egui::TextureId>>` を追加し、`loaded.ir.textures` と並列のサムネイル TextureId キャッシュを保持。既存の `pkg_thumb_cache`（UnityPackage 内テクスチャ用）と同じ 64px サムネイルパイプラインを流用。
- `rebuild_ir_thumb_cache` / `append_ir_thumb_cache` / `clear_ir_thumb_cache` / `sync_ir_thumb_cache` の 4 メソッドを追加。`sync` は `loaded.ir.textures.len()` と cache 長を比較し、増加時のみ append、減少時は rebuild、`loaded` が無い時は clear する差分更新を行う。
- `assign_texture_core` の新規テクスチャ push 経路で直接 `build_ir_thumb_entry` を呼び、`&mut self` 再取得による borrow 衝突を回避しつつサムネイルを同期追加。
- `show_material_editor_window` の先頭で `sync_ir_thumb_cache` を呼び、モデル切替・BG ロード完了など外部経路で `ir.textures` 長が変化しても UI 表示が追従する設計。
- 共通 `texture_slot_widget()` ヘルパー関数を追加。各セクションから呼び出し、`(assign_clicked, reset_clicked)` の bool ペアを返して呼び出し側で `pending_tex_request` / `pending_tex_clear` を設定する借用境界設計。

### バグ修正（リリース前レビュー対応）

- **[review_01 P1] モデル切替後に前モデルのサムネイルが残る問題を修正** — `finish_load_with_gpu` / `cancel_gpu_build` / `cancel_bg_index_load` で `clear_ir_thumb_cache()` を呼び出すようにした。前モデルと新モデルのテクスチャ数が一致した場合、`sync_ir_thumb_cache()` が長さ比較だけで early return するため、前モデルの `TextureId` がそのまま再利用されて別モデルのサムネイルが誤って表示される問題があった。
- **[review_01 P2] PSD→PNG 変換後にサムネイルが更新されない問題を修正** — `poll_pending_psd_conversions()` で PNG 差し替え完了時に該当 `tex_idx` の `TextureId` を再生成するようにした。`sync_ir_thumb_cache()` は長さが変わらないため再構築されず、PSD デコード失敗で初期サムネイルが `None` だったケースでは、PNG 変換完了後も永続的に空欄のまま残る問題があった。
- **[review_02 P1] 材質編集ウィンドウ未表示時のテクスチャ追加で index がずれる問題を修正** — `assign_texture_core()` / `apply_tex_preview()` の新規テクスチャ push 経路で、単純な `cache.push()` ではなく「不足分を末尾から一括 append」するロジックに変更。`ir_thumb_cache` は材質編集ウィンドウを一度開くまで長さ 0 のままのため、従来の push だけでは新規サムネイルが `cache[0]` に入り、既存スロットの表示と index がずれる問題があった（モデル読込直後にメイン UI から BaseColor を差し替えるだけで全スロットの表示が壊れる）。

## v0.5.1（2026-04-13）

VRM 1.0 Expression 材質バインド再生、補助テクスチャスロットの永続化、材質編集ドロワー UX 改善。

### 新機能

- **Expression 材質バインド（VRM 1.0）** — VRM 1.0 Expression の `materialColorBinds` / `textureTransformBinds` をビューアが再生時に正しく処理するようになった。6 種のカラー対象（`color` / `emissionColor` / `shadeColor` / `matcapColor` / `rimColor` / `outlineColor`）と UV スケール/オフセットが、VRM 1.0 仕様のアルゴリズム `finalValue = baseValue + Σ((targetValue − baseValue) × weight)` に従って複数 Expression 間で加算合成される。ベース値はロード時にキャプチャされ、材質エディタで編集すると更新されるため、エディタ編集後の値が新しいベースとして Expression のブレンド基準になる。
- **Sphere / Toon テクスチャスロット編集** — 材質編集ドロワーの「テクスチャスロット」セクションに MMD 固有の `Sphere` / `Toon` スロットを追加。既存の 8 補助スロットと同じ UI パターン（ファイルダイアログボタン + `×` リセットボタン）。
- **補助テクスチャスロットの永続化** — BaseColor 以外の全 10 スロット（Emissive / Normal / Shade / ShadingShift / RimMultiply / OutlineWidth / Matcap / UvAnimMask / Sphere / Toon）のテクスチャ割当が `popone_history.json` に保存されるようになった。`TextureHistoryEntry` に `slot` フィールドを追加。以前は BaseColor のみ永続化され、補助スロットは再起動で消失していた。新しい `slot` フィールドは `#[serde(default)]` により、v0.5.0 の履歴ファイルは `BaseColor` として解釈される（後方互換）。逆に v0.5.1 で保存した履歴を v0.5.0 で読み込んでも unknown field は無視されるため、両方向互換性を確保。
- **材質編集ドロワーのダーティインジケータ** — 編集中の材質にパラメータ編集差分・BaseColor テクスチャ割当・補助スロットテクスチャ割当のいずれかがあるとき、ウインドウタイトルの末尾に `*` を表示する。これにより「触った材質」を一目で識別できる。
- **材質パラメータのコピー / ペースト** — 材質編集ドロワーのツールバー行に「コピー」「ペースト」ボタンを追加。コピーは `diff_from(pristine, current)` の結果をセッションローカルのクリップボードに保存。ペーストは編集中の材質に通常の dirty 追跡フローで適用する。テクスチャ割当はパス依存を避けるため意図的に除外しており、カラー/スカラー値のみが材質間で転送される。
- **PMX 非対応バッジの視覚強化** — リム / MatCap / UV アニメの CollapsingHeader は以前はセクションタイトルに `(PMX非対応)` が plain text で埋め込まれていた。タイトルはクリーンにし、各セクション本文の先頭に色付きの `⚠ PMX 非対応` バッジを配置。ホバーツールチップに「この項目は PMX 出力では反映されません。MME (.fx) 出力やビューアプレビューでは反映されます。」を表示。

### パフォーマンス

- **DrawCall uniform バッファ最適化** — `DrawCall` が `UNIFORM | COPY_DST` 属性の永続的な `wgpu::Buffer`（`material_buf`）を保持するようになった。`create_material_bind_group` を `serialize_material_uniform` / `create_material_buffer_and_bind_group` / `write_material_buffer` の 3 関数に分割。既存の bind group 再生成経路では uniform-only 更新に `queue.write_buffer` を使い、材質編集や Expression フレームごとの bind group 全再生成を回避する。これが GPU リソースチャーンなしで per-frame Expression 材質反映を実現する基盤となる。

### 内部実装

- `IrMorphKind` に `Material { color_binds, uv_binds }` バリアントを追加（既存の `Vertex` / `Group` と並列）。VRM Expression が頂点モーフと材質バインドの両方を持つ場合は同名の 2 つの IrMorph として発行し、既存の名前ベース `morph_weights` マッピングで両方に同一ウェイトが適用される（Compound バリアント不要）。
- `intermediate::types` に `MaterialColorBindType`、`IrMaterialColorBind`、`IrTextureTransformBind` の新型を追加。`MaterialColorBindType::from_vrm_str` で VRM 1.0 の `type` 文字列を enum にパース。
- `GpuMorphEntry::Material` バリアント + `GpuModel` に `MaterialBaseValues` フィールドを追加。アクティブな材質モーフを走査してカラー/UV 差分を材質ごとに蓄積し、dirty な材質のみ `Vec<Option<MaterialParams>>` を返す純関数 `accumulate_expression_materials()` を追加。
- `IrModel::merge()` が `IrMorphKind::Material` の `color_binds` / `uv_binds` 内 `material_index` を結合時にオフセット調整するようになった。

### バグ修正（リリース前レビュー対応）

Codex による 5 回のレビューラウンドで、新規の Expression 材質バインド経路と履歴呼出フロー / 既存の材質エディタとの統合問題が複数指摘され、全てリリース前に解消:

- **テクスチャ履歴呼出の順序修正** — 旧実装はテクスチャ復元 → pristine 復元の順で、2 ステップ目が補助スロットのテクスチャ参照（`emissive_texture` / `normal_texture` 等）を破壊していた。修正: pristine 復元を先に実行し、その後にテクスチャ復元 + param override を適用。pristine 復元と同時に `tex.assignments` / `tex.pkg_assignments` / `slot_texture_paths` もクリアして「呼出 = 保存時点の完全再現」を保証。
- **Expression ウェイト 0 復帰時の GPU 残留** — `accumulate_expression_materials` が `weight.abs() < 1e-6` の morph を完全にスキップしていたため、`1.0 → 0.0` のフレームでベース値が書き戻されず、最後に適用された色/UV が GPU 側に残留していた。修正: `GpuMorphEntry::Material` が参照する全材質を事前に dirty 扱いにすることで、weight=0 のとき accum がゼロ → `base + 0 = base` が書き戻される挙動に。
- **材質エディタ編集値が Expression のベースにならない** — `material_base_values` がモデルロード時に一度だけキャプチャされており、エディタ編集値が Expression の合成基準に反映されなかった。修正: `apply_pending_material_rebuilds` で dirty 時に `MaterialBaseValues::from_ir(mat)` を再キャプチャ。
- **手動モーフ中の編集で Expression 材質反映が消える** — 手動モーフスライダーで非ゼロ weight を保持しているときに材質エディタを触ると、Expression の材質反映が消失し、モーフを再度動かすまで戻らなかった。修正: `apply_pending_material_rebuilds` の末尾で、いずれかの morph weight が非ゼロなら Expression 材質反映を再実行。
- **full rebuild で BaseColor bind が再生成されない** — `rebuild_material_bind_groups` は `material_buf` / aux / MMD bind group のみを更新しており、標準パスの `texture_bind_group` は未更新だった。pristine 復元後に古い BaseColor テクスチャ bind が画面に残る不具合があった。修正: full rebuild パスで `texture_bind_group` も再生成。
- **PMX/PMD 材質の BaseColor bind 消失（上記修正の回帰）** — 最初の修正では `mat.base_color_tex_info` のみを参照していたため、PMX/PMD 材質は `texture_index` を持つが `base_color_tex_info` が None のため、full rebuild で texture_bind_group が None になり白テクスチャに後退していた。修正: 初回 DrawCall 構築と同じ情報源（`mat.texture_index` 優先、sampler は `base_color_tex_info.sampler` → デフォルトフォールバック）に揃えた。
- **可視材質 export で Material morph の material_index 再マップ漏れ** — `build_filtered_ir()` は mesh の `material_index` を `mat_remap` で詰め直していたが、`IrMorphKind::Material` の bind 内 `material_index` は clone のまま残留していた。修正: `color_binds` / `uv_binds` を `filter_map + mat_remap` で再マップし、除外材質を参照する bind は落とす。
- **空 Material morph が可視材質 export 後に残留** — 可視材質フィルタで bind 先が全除外された Material morph は、頂点依存しないという理由で常に生存扱いされ、機能しない「死んだ表情」として出力に残っていた。修正: Phase 3（morph_alive 判定）で Material morph の生存判定を「bind が 1 つでも `mat_remap` に残るか」に変更。Group モーフの収束ループがカスケードで孤立 Group も自然に除外。

### テスト

- ユニットテスト 235 件（v0.5.0 の 230 件から増加）。追加カバレッジ: `MaterialColorBindType::from_vrm_str` の 6 種の正当文字列 + 不明/空文字列フォールバック、`IrModel::merge()` における Material morph の material_index オフセット、`TextureHistoryEntry` の slot フィールド serde（後方互換デフォルト、明示指定、ラウンドトリップ）。

### v0.6.0 に延期

- 残りテクスチャスロット（RimMultiply / OutlineWidth / Matcap / UvAnimMask）の UV 変形編集 UI — 既存コードに UV 変形編集 UI 自体がまだ存在しないため、想定以上の実装スコープ。
- 材質編集ドロワー開時の D&D スロット選択ダイアログ。
- 自動割当のスロットヒントマッチ（`*_normal*` → Normal 等）。
- テクスチャスロットのサムネイルプレビュー。
- セクション折りたたみ状態のセッション間永続化。
- ユーザーカスタムプリセット保存 / 読込。
- sdPBR `.fx` 生成、MaskedMaterial 対応、MME `.fx` 読み込み。


## v0.5.0（2026-04-13）

MToon / lilToon の全パラメータを GUI で編集できる材質編集ドロワーと、MME（ray-mmd）向け `.fx` マテリアル生成機能を追加。

### 新機能

- **材質編集ドロワー** — 材質行の「編」ボタンから独立フローティングウインドウ（`egui::Window`）を開く。セクション構成: 基本（diffuse / alpha mode / baseColor テクスチャ）、影（shade_color / shading_toony / shading_shift ＋テクスチャ）、アウトライン（edge_color / 幅モード / outline width テクスチャ）、リム（パラメトリックリム / rim multiply テクスチャ）、MatCap、UV アニメ、エミッシブ / 法線、その他、MME 出力プレビュー。
- **MToon / lilToon 全パラメータ編集** — 25 個以上のカラー / スカラーと、補助テクスチャスロット（normal / emissive / shade / shadingShift / rim / outline / matcap / uvAnimMask）をすべて GUI 編集可能。編集値は標準描画パスと MMD 互換描画パスの両方に即時反映される。
- **スロット単位・材質単位のリセット** — 各スロットカードに `×` ボタンを設置してそのスロットだけを消去可能。材質単位の「初期値に戻す」はロード時点のスナップショットから復元する。
- **組み込みプリセット** — MToon 1.0 デフォルト / lilToon 標準 / PMX 互換 の 3 種類を同梱。
- **材質編集の永続化** — 材質ごとの編集差分（カラー / スカラー差分 ＋ MME カテゴリ上書き）を `popone_history.json` に保存し、リロード時に復元する。
- **MME（ray-mmd）マテリアル生成** — 出力タブの PMX 変換セクションに「MME マテリアル (.fx) も出力」チェックボックスを追加。チェックを入れると PMX 変換時に `<モデル名>_mme/material_<名称>.fx` を書き出す。`CUSTOM_ENABLE` ベースのテンプレート（Standard / Skin / HairAniso / Glass / Cloth / ClearCoat / Emissive）で生成。ray-mmd ルートフォルダはチェックボックス展開時にフォルダ選択ダイアログで設定可能（未設定時はカレントディレクトリ `.\` をフォールバック）。`#include` パスは `pathdiff` + `dunce` 正規化で相対化し、計算不能時はフォールバック。PMX では扱えない補助テクスチャ（normal / emissive / matcap / rim / shading shift）は `<モデル名>_mme/textures/` にコピーして相対パスで参照する。`.fx` と `README.txt` は Shift-JIS + CR+LF でエンコード。`#include` 先（`material_common_2.0.fxsub`）が存在しない場合は変換結果に警告を表示（ファイル出力は継続）。

### 挙動の変更

- **PMX 変換の判定主軸を `ShaderFamily::Mtoon` に切替**。旧来の `is_mtoon()`（`mtoon.is_some()` 判定）は材質編集 UI が非 MToon 材質にも MToon パラメータを表示するため、安直に使うと PMX 出力の ambient / specular が MToon 扱いに変わってしまう。副作用回避のため、UI から明示的に「MToon 有効化」チェックを入れるまで `shader_family` は変更しない。

### テスト

- ユニットテスト 230 件（v0.4.0 の 185 件から増加）。追加カバレッジ: `MaterialParamOverride` の diff/apply ラウンドトリップ、`RayMmdMaterialKind` カテゴリ推定（日本語 / 大小混在 / プレフィックス付き材質名）、`generate_fx` のセクション完全性と CR+LF エンコーディング、`TextureSlot::is_linear` の全バリアントカバレッジ。


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
