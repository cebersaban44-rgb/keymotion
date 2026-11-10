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
1. Dört kontrol geçmeli: fmt, clippy, testler ve typecheck.
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

CI eklendiğinde fmt, clippy, test ve typecheck kontrolleri iki dalda da birleştirme şartı olacak.

## Varsayılan dal

GitHub'da varsayılan dal **`main`**'dir, yani ziyaretçi kararlı sürümün kodunu görür. Dışarıdan gelen PR'lar `develop`'u hedeflemelidir; bu, açık kaynak hazırlığında `CONTRIBUTING.md`'ye yazılacak. O aşamada varsayılan dalın `develop` olup olmayacağına yeniden bakılacak.

## Kim ne yapar

- **Asistan (Claude):**
  - Her işi ayrı bir dalda yapar.
  - Kontrolleri çalıştırır ve gerekirse cihazda dener.
  - PR açar ve açıklamasını yazar.
  - `main`'e hiçbir zaman doğrudan yazmaz.
- **Proje sahibi:**
  - PR'ları inceler ve onaylar. Küçük işlerde birleştirmeyi asistana bırakabilir.
  - Sürümün `main`'e çıkışına ve yayına karar verir.
