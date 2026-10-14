# @willh/pdf-encrypt

`pdf-encrypt` 是以 Rust 撰寫的命令列工具，用來加密、解密及讀寫 PDF 文件屬性。此 npm 套件會依作業系統與 CPU 自動安裝對應的預先編譯執行檔，不需要 Rust 開發環境，也不會在安裝時另外下載檔案。

## 安裝

```bash
npm install -g @willh/pdf-encrypt
pdf-encrypt --version
```

不安裝直接執行：

```bash
npx @willh/pdf-encrypt --help
```

支援平台：macOS (arm64、x64)、Linux (arm64、x64，glibc 與 musl 皆可)、Windows (arm64、x64)。

## 使用範例

```bash
# 以 AES-256 加密，設定開啟密碼
pdf-encrypt encrypt -i input.pdf -o output.pdf --open-password secret

# 解密
pdf-encrypt decrypt -i output.pdf -o plain.pdf --password secret

# 查詢文件屬性
pdf-encrypt info -i input.pdf --json
```

完整說明請執行 `pdf-encrypt --help`，或參考 [GitHub 專案](https://github.com/doggy8088/pdf-encrypt)。

## 授權

MIT
