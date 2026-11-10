# Klavye Stüdyo

AJAZZ AK980 PRO (ve ileride benzer SONiX tabanlı klavyeler) için Windows kontrol uygulaması. Rust çekirdek, Tauri 2 kabuğu, React arayüz.

> Durum: geliştirme aşamasında. Neredeyiz, neler yapıldı ve sırada ne var: **[docs/DURUM.md](docs/DURUM.md)**.

## Hızlı başlangıç

```bash
cargo test --workspace          # testler, TS tipleri ve mock verileri üretir
pnpm --dir app install
pnpm --dir app dev              # arayüz sunucusu (127.0.0.1:1420)
cargo build -p klavye-studio
./target/debug/klavye-studio.exe
```

Ayarlar (ışık, tuş atama, ekran) için klavye **USB kabloyla** bağlı ve arkadaki mod anahtarı **ortada (USB)** olmalı. Bluetooth üzerinden yalnızca pil okunur.

## Belgeler

- [docs/DURUM.md](docs/DURUM.md): durum, kronoloji, dersler, yapılacaklar
- [docs/donanim-dogrulama.md](docs/donanim-dogrulama.md): cihazda ölçülen protokol gerçekleri
- [docs/mimari.md](docs/mimari.md): mimari kararı
- [docs/research/](docs/research/): protokol ve ekran araştırmaları

## Güvenlik

Uygulama yalnızca doğrulanmış komutları gönderir. Bootloader ve firmware güncelleme dizileri ile Bluetooth OTA kanalı kodda engellidir. Ayrıntı için DURUM.md §4'e bakın.
