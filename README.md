# fuel-usb-uploader

USB に挿した `UTRAN.CSV` を rust-fuel-worker へ自動でアップロードする Windows 常駐アプリです (Refs yhonda-ohishi/nginx#814)。

送り先は auth-worker の機械用の入口 `POST https://auth.ippoan.org/m/fuel-utran/uploads` です。machine credential の Basic 認証で CSV をそのまま送り、auth-worker が rust-fuel-worker に中継して MariaDB に取り込みます。

## インストール

1. [Releases](https://github.com/ohishi-exp/fuel-usb-uploader/releases) から `fuel-usb-uploader-<版>-x64.msi` を取ってきます (隣の `.sha256` で照合できます)
2. MSI を実行します。管理者の権限 (UAC) が要ります
   - **署名していない**ので、SmartScreen が「Windows によって PC が保護されました」と出します。社内で配る MSI なので、「詳細情報」→「実行」で進めてください
3. 置かれる場所
   - 本体: `C:\Program Files\fuel-usb-uploader\fuel-usb-uploader.exe`
   - 自動起動: `HKLM\Software\Microsoft\Windows\CurrentVersion\Run` の `fuel-usb-uploader` (この PC の全ユーザーのログオン時に起動します)
   - スタートメニュー: 「給油データ USB アップローダー」(トレイから終了したあとに起動し直す用)
4. インストールした直後は起動しません。スタートメニューから起動するか、ログオンし直してください

上書きインストールで新しい版に入れ替わります (MajorUpgrade)。アンインストールは「設定」→「アプリ」から。

## 資格情報の発行 (管理者)

PC ごとに machine credential を 1 つ発行します。admin の JWT で次を送ります:

```http
POST https://auth.ippoan.org/api/machine-credentials
Authorization: Bearer <admin の JWT>
Content-Type: application/json

{"purpose":"fuel-utran","label":"<PC 名>"}
```

応答 (201) の `username` (`mc_` + 16 桁の英数字) と `password` (24 文字) を PC の利用者に渡します。

- **password が見えるのは発行時の応答 1 回だけです。** サーバーには hash しか残りません
- 紛失したら `POST https://auth.ippoan.org/api/machine-credentials/<username>/rotate` で同じ username のまま新しい password を出し、PC 側で入れ直します
- PC を廃棄したら `POST https://auth.ippoan.org/api/machine-credentials/<username>/revoke` で無効にします

## 初回の設定

1. 初めて起動すると、Windows 標準の資格情報のダイアログが出ます。発行された username と password を入れます
   - 形式が違う (username が `mc_` + 16 桁でない、password が 24 文字でない) と、もう一度聞きます
   - 入力は Windows の資格情報マネージャー (「Windows 資格情報」→ 汎用資格情報 `fuel-usb-uploader`) に保存されます。ファイルやログには書きません
2. あとから変えるときは、タスクトレイのアイコン (青い丸に白いしずく) を右クリック →「資格情報を設定」

## 動作の流れ

1. 2 秒おきにリムーバブルドライブ (USB メモリなど) の一覧を見て、**新しく挿されたドライブ**を見つけます (起動時に挿さっていたドライブも 1 回見ます)
2. そのドライブの**直下**にある `UTRAN.CSV` (大文字小文字は問いません) を、最大 10 秒、1 秒おきに探します。無ければ何もしません
3. 5 MB を超えるファイルは送らずに通知します
4. CSV をそのまま送ります。結果をトーストで通知します
   - 成功: 「給油データを取り込みました: 追加 12 件 / 既存 3 件」(同じ内容を送り直しても「既存」に数えられるだけで、重複はしません)
   - CSV の誤り (400): 何行目が読めないかを通知します
   - 資格情報の誤り (401): ダイアログを出し直し、入れ直したら 1 回だけ送り直します
   - サーバーが一時的に使えない (503)・つながらない: 5 秒・15 秒・45 秒あけて最大 3 回やり直します。それでも駄目なら通知します
5. トレイの「今すぐ確認」は、いま挿さっているリムーバブルドライブを全部見ます (挿しっぱなしの USB を送り直すとき)

通知の送り元は「Windows PowerShell」と表示されます (アプリの ID を登録していないため)。

USB の外付け HDD・SSD のように Windows が「ローカル ディスク」(固定) と扱うドライブは対象外です。

## 設定ファイル (任意)

送り先を変えるときだけ `%APPDATA%\fuel-usb-uploader\config.toml` を置きます (無ければ既定値)。

```toml
endpoint = "https://auth.ippoan.org/m/fuel-utran/uploads"
```

`https://` 以外は受け付けません。書式が誤っていると起動時に通知し、既定の送り先を使います。

## トラブルのとき

- ログ: `%LOCALAPPDATA%\fuel-usb-uploader\log.txt` (1 MB を超えると `log.old.txt` に回します)
  - 1 行に日時・ドライブ・結果だけを書きます。例:

    ```text
    2026-10-18 12:00:00	E:	imported inserted=12 skipped=3
    2026-10-18 12:05:10	F:	bad_csv error=csv line=4
    2026-10-18 12:06:00	E:	unauthorized
    ```

  - 結果の種類: `imported` / `bad_csv` / `unauthorized` / `too_large` / `file_too_large` / `unavailable` / `network` / `unknown status=<HTTP>` / `not_found` / `read_failed` / `no_credential`
- トレイにアイコンが無い: スタートメニューから起動します (2 つ目は起動しても何もせず終わります)
- `unauthorized` が続く: 管理者に rotate してもらい、「資格情報を設定」から入れ直します

## 開発

- `core/` (`fuel-usb-core`): OS に依存しない判断 (ドライブの差分・`UTRAN.CSV` の探索・応答の解釈・やり直し・通知文・設定)。Linux で `cargo test -p fuel-usb-core`
- `app/` (`fuel-usb-uploader`): Windows 専用 (トレイ・USB の検知・送信・資格情報・トースト・ログ)。Windows 以外では「Windows 専用です」と出して終わります
- CI (`.github/workflows/ci.yml`): ubuntu で core の fmt・clippy・test、windows-latest で app の clippy と release build
- Release (`.github/workflows/release.yml`): `Cargo.toml` の `workspace.package.version` を上げてから tag `v<x.y.z>` を push すると、MSI を作って Release に添付します (手動実行もできます)

手元で MSI を作るとき (Windows、repo の根で):

```powershell
cargo build --release -p fuel-usb-uploader
dotnet tool install --global wix --version 5.0.2
wix build wix/main.wxs -arch x64 -d Version=0.1.0 -o fuel-usb-uploader-0.1.0-x64.msi
```
