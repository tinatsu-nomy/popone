<!-- START doctoc generated TOC please keep comment here to allow auto update -->
<!-- DON'T EDIT THIS SECTION, INSTEAD RE-RUN doctoc TO UPDATE -->
**Table of Contents**  *generated with [DocToc](https://github.com/thlorenz/doctoc)*

- [更新履歴](#%E6%9B%B4%E6%96%B0%E5%B1%A5%E6%AD%B4)
  - [v0.3.0（未リリース）](#v030%E6%9C%AA%E3%83%AA%E3%83%AA%E3%83%BC%E3%82%B9)

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

# 更新履歴

[English](CHANGELOG.md)

## v0.3.0（未リリース）

初回公開リリースのベースライン。予定項目の詳細は [ROADMAP](ROADMAP.jp.md) を参照してください。

主な検討項目:

- **Expression 材質バインド** — VRM 1.0 Expression 再生時の `materialColorBinds` / `textureTransformBinds`（シェーダー側は対応済み、Expression パイプラインの接続が必要）
- **Unity `.anim` 残課題** — `HumanTrait` 準拠の正確な Muscle 角度範囲、足首/手/指の軸検証、Foot IK、表情カーブ対応
- **インポートオプション** — OBJ / STL の単位・座標系選択 UI の強化
- **バックグラウンドロード内部整備** — `CpuParseInput` の `ArchiveEntry` / `Reload` バリアント追加
