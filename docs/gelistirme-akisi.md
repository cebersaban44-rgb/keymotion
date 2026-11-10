# Geliştirme akışı: dallar, PR'lar ve sürümler

Karar tarihi: **2026-11-10**. Projede kod nasıl ilerler, ne zaman ve nasıl yayına çıkar, burada yazıyor. Kural değişirse önce bu belge güncellenir.

## Özet

```
feature/… ─┐
fix/…     ─┼─▶ develop ──(sürüm PR'ı)──▶ main ──▶ etiket v0.2.0 ──▶ GitHub Releases (.exe)
docs/…    ─┘                               ▲
hotfix/… ──────────────────────────────────┘  (sonra main → develop'a da alınır)
```

- **`main` yayındaki sürümdür (production).** Kullanıcılar uygulamayı dalın kendisinden değil, **Releases** sayfasındaki kurulum dosyasından indirir. `main`'e yapılan her birleştirme bir sürümdür ve bir etiketle işaretlenir.
- **`develop` günlük geliştirme dalıdır.** Her iş kısa ömürlü bir dalda yapılır ve PR ile `develop`'a girer.
- `main`'e doğrudan commit atılmaz.

## Dallar

| Dal | Ne için | Nereden açılır | Nereye birleşir |
|---|---|---|---|
| `main` | Yayındaki sürüm | — | — |
| `develop` | Bir sonraki sürümün biriktiği yer; her zaman derlenir ve testleri geçer | `main` | `main` (sürüm PR'ıyla) |
| `feature/<konu>` | Yeni özellik | `develop` | `develop` |
| `fix/<konu>` | Henüz yayınlanmamış koddaki hata | `develop` | `develop` |
| `docs/<konu>` | Yalnızca belge | `develop` | `develop` |
| `chore/<konu>` | Araç, bağımlılık, CI, sürüm hazırlığı | `develop` | `develop` |
| `hotfix/<konu>` | **Yayındaki** sürümde acil hata | `main` | `main`, ardından `develop` |

Dal adları kısa, küçük harfli ve İngilizce olur; kelimeler tireyle ayrılır. Örnekler: `feature/keymap-editor`, `fix/mirror-gamma`, `docs/branching-and-release-guide`. İş bitince dal silinir.

## Günlük iş

```bash
git switch develop && git pull
git switch -c feature/kisa-konu
# … değişiklikler …
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir app typecheck
git push -u origin feature/kisa-konu
gh pr create --base develop
```

**PR açmadan önce:**
1. Dört kontrol geçmeli: fmt, clippy, testler ve typecheck. CI bunları her PR'da yeniden çalıştırır (aşağıda); yerelde çalıştırmak beklemeyi kısaltır.
2. Davranışı değiştiren her iş **gerçek uygulamada** denenmeli (DURUM.md §5, CDP).
3. Klavyeye yazan bir akış değiştiyse cihazda doğrulanmalı ve sonucu `docs/donanim-dogrulama.md`'ye tarihli yazılmalı.

**PR açıklaması:** Ne değişti, neden değişti ve nasıl denendi. Kullanıcıya görünen bir değişiklikse `CHANGELOG.md`'ye de bir satır eklenir (dosya ilk sürümle birlikte gelecek).

## Commit'ler

- Mesajlar İngilizce yazılır. İlk satır emir kipinde ve en fazla 72 karakterdir; gövdede "neden" anlatılır. Örnek: `Fix per-key frames leaving unpainted keys lit`.
- Commit'ler GitHub'ın noreply e-postasıyla atılır (depoya özel `user.email`). Gerçek e-posta public geçmişe girmez.
- Kişisel veri commit'lenmez: Bluetooth MAC adresi, kullanıcı adı içeren yerel yollar, anahtarlar.

## Birleştirme yöntemi

| PR | Yöntem | Neden |
|---|---|---|
| `feature/fix/docs/chore` → `develop` | **Squash** | Her PR `develop`'ta tek, okunur bir commit olur. |
| `develop` → `main` (sürüm) | **Merge commit** | Squash yapılırsa `main` ile `develop`'un geçmişi ayrışır ve sonraki sürümlerde çakışma çıkar. |
| `hotfix/…` → `main` | **Merge commit** | Aynı neden. Ardından `main`, PR ile `develop`'a alınır. |

## Sürüm çıkarma

Sürüm numaraları [SemVer](https://semver.org/lang/tr/) kurallarına uyar. Kararlı 1.0'a kadar `0.x.y` kullanılır:
- Yeni özellikler `x`'i artırır (0.2.0).
- Hatalar ve küçük düzeltmeler `y`'yi artırır (0.2.1).

Sürüm numarası üç yerde tutulur ve hepsi aynı olmalıdır:
- `Cargo.toml` (`[workspace.package] version`)
- `app/package.json`
- `app/src-tauri/tauri.conf.json`

**Adımlar:**
1. `chore/release-v0.x.0` dalında sürüm numaraları ve `CHANGELOG.md` güncellenir, PR ile `develop`'a alınır.
2. `develop` → `main` PR'ı açılır. Başlığı "Release v0.x.0" olur ve merge commit ile birleştirilir. **Sürüm her zaman proje sahibinin onayıyla çıkar.**
3. `main` üzerinde etiket atılır:

   ```bash
   git switch main && git pull
   git tag -a v0.x.0 -m "v0.x.0"
   git push origin v0.x.0
   ```

4. GitHub Release oluşturulur: CHANGELOG'daki notlar ve NSIS kurulum dosyası (.exe). İleride CI, etiketten kurulumu otomatik derleyip imzalayacak (DURUM.md P3).

## Acil düzeltme (hotfix)

Yayındaki bir sürümde, bir sonraki sürümü bekleyemeyecek bir hata çıkarsa:

1. `main`'den `hotfix/<konu>` dalı açılır, düzeltme ve yama sürümü (örneğin 0.2.1) hazırlanır.
2. PR ile `main`'e alınır (merge commit), etiket atılır ve Release çıkar.
3. `main` → `develop` PR'ı ile düzeltme `develop`'a da alınır, böylece bir sonraki sürümde kaybolmaz.

## GitHub koruma kuralları

| Dal | Kural |
|---|---|
| `main` | PR zorunlu (tek geliştirici olduğu için onay sayısı 0). Force-push ve silme yasak. Kural yöneticiler için de geçerli. |
| `develop` | PR zorunlu. Force-push ve silme yasak. Depo sahibi gerekirse kuralı atlayabilir. |

### CI (otomatik kontroller)

`.github/workflows/ci.yml`, `develop` ve `main`'e açılan her PR'da ve bu dallara yapılan her birleştirmede Windows üzerinde çalışır. Adımları:

1. Arayüz: bağımlılıkları kurar, typecheck ve derleme (`pnpm build`) yapar.
2. Rust: rustfmt, clippy (`-D warnings`) ve `cargo test --workspace` çalıştırır.
3. Bindings kontrolü: `cargo test` TypeScript tiplerini Rust'tan yeniden üretir. `app/src/bindings` commit'lenmiş halinden farklıysa ya da commit'lenmemiş yeni dosya varsa kontrol kırmızı olur.

"Build and test" kontrolü iki dalda da **birleştirme şartıdır**: kırmızı PR birleştirilemez.

Depodaki `.gitattributes` bütün metin dosyalarını LF satır sonuyla tutar. Böylece Windows'ta da rustfmt, üretilen tipler ve CI aynı baytları görür.

## Varsayılan dal

GitHub'da varsayılan dal **`develop`**'tur (karar 2026-11-10). Nedenleri:

- Fork'lar ve dışarıdan gelen PR'lar kendiliğinden en güncel koda ve doğru dala gider. `main`'e açılan PR'ları tek tek `develop`'a çevirmek ve eski koda göre yazılmış PR'lardaki çakışmalarla uğraşmak gerekmez.
- GitHub'ın "Fixes #12" gibi otomatik issue kapatma kelimeleri yalnızca varsayılan dala giren PR'larda çalışır.
- Kullanıcılar uygulamayı koddan değil, Releases sayfasından kurar. Kararlı sürüm, etiketler ve Releases ile bulunur; README'nin en üstünde son sürüme bağlantı olur.

Bir düzeltme `develop`'a girince issue kapanır ama kullanıcıya bir sonraki sürümle ulaşır. Bunu görünür kılmak için her issue bir sürüme (milestone, örneğin `v0.3.0`) bağlanır ve sürüm notlarında kapanan issue'lar listelenir.

## Kim ne yapar

- **Asistan (Claude):**
  - Her işi ayrı bir dalda yapar.
  - Kontrolleri çalıştırır ve gerekirse cihazda dener.
  - PR açar ve açıklamasını yazar.
  - `main`'e hiçbir zaman doğrudan yazmaz.
- **Proje sahibi:**
  - PR'ları inceler ve onaylar. Küçük işlerde birleştirmeyi asistana bırakabilir.
  - Sürümün `main`'e çıkışına ve yayına karar verir.
