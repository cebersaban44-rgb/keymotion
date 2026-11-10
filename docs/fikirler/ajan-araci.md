# Fikir: Ajanlar için klavye aracı

**Durum:** Açık fikir, şimdi yapılmayacak. Uygulamanın bütün özellikleri (P3 ürünleştirme dahil) bitince ele alınacak. `DURUM.md` §7, "Sonraya bırakılan fikirler".

**Kaynak:** Kullanıcının fikri, 2026-11-10. Aşağıdaki "İlk düşünceler" Claude'un aynı gün yazdığı öneri. Uygulamaya geçmeden önce kullanıcıyla yeniden konuşulacak.

## 1. Kullanıcının fikri

- Klavye artık görsel olarak birçok işe yarıyor:
  - tuş ışıkları (efektler, tuş başına renk, canlı katman);
  - dalgalar ve geçişler;
  - ekran kartları ve görseller;
  - odak zamanlayıcısı.
- Claude Code, Codex gibi kodlama ajanları da bunları kullanabilmeli:
  - Ekrana kısa bir metin ya da görsel göndermek.
  - Onay, ret, hata, bekleme ya da duygu durumu gibi bildirimleri renk ve dalgayla vermek.
  - Odak modunu başlatıp takip etmek.
- Kullanıcılar için uygulama zaten var. Ajanlar için de ayrı bir araç gerekiyor ve bu araç ajanlar için "doğal", rahat kullanılır olmalı.
- **Açık soru:** MCP mi, CLI mı, yoksa başka ve daha iyi bir yol mu?

## 2. İlk düşünceler

### 2.1 Tek çekirdek: klavyeyi yine uygulama sürer

- Klavyeye tek bir süreç yazmalı: Klavye Stüdyo'nun cihaz aktörü (`device.rs`).
- İki ayrı program aynı anda HID'e yazarsa çerçeveler karışır. Ayrıca uygulamanın kurduğu her şey devre dışı kalır:
  - canlı katmanlar (`SetLive`, `SetAmbient`);
  - yükleme bütçesi;
  - "Ekranda ne görünür?" sırası;
  - güvenlik korumaları (firmware, bootloader ve OTA yasakları).
- Bu yüzden uygulama **yerel bir API** açar. Erişim yalnızca bu bilgisayardan olur (adlandırılmış kanal ya da `127.0.0.1` ve bir anahtar). Ajanlara yönelik her araç bu API'nin ince bir yüzü olur.
- **Ön şart:** Uygulama kapalıyken de çalışmalı. P3'teki arka plan servisi (sistem tepsisi, otomatik başlatma) bunun için gerekli. Bir seçenek de aracın uygulamayı arka planda başlatması.

### 2.2 Üç yüz, üç ayrı iş

| Yüz | Ne için | Neden |
|---|---|---|
| **Ajan kancaları** (hooks) | Ajanın durumu, ajan hiçbir şey yapmadan: izin bekliyor, iş bitti, hata | En "doğal" yol. Ajanın karar vermesi ya da token harcaması gerekmez, her zaman çalışır. Örnekler: Claude Code'un kancaları (bildirim, durma, araç çağrısından önce), Codex'in bildirim (`notify`) ayarı. Hazır kanca dosyaları tek komutla kurulur. (Olay adları uygulama zamanında doğrulanacak.) |
| **CLI** (`kb` genişler) | Her ajan, betik, git kancası, CI | Kurulum istemez, her kabukta çalışır. Kancalar da bunu çağırır. Örnek komutlar: `kb signal waiting`, `kb say "Testler geçti"`, `kb focus start 25`. |
| **MCP sunucusu** | Ajanın kendi seçtiği ifadeler: "kullanıcıya şunu göster", "odak modunu başlat" | Ajan, araç listesinden ve şemasından neyin mümkün olduğunu kendisi görür. Claude Code, Codex, Cursor gibi istemcilerde ortak. |

- Ek olarak kısa bir talimat dosyası olur (AGENTS.md parçası ya da beceri/skill). Ajana ne zaman sinyal göndereceğini, ne zaman **göndermeyeceğini** anlatır; gereksiz ışık ve yükleme olmasın.
- **Önerilen sıra:**
  1. Önce yerel API, CLI ve kancalar. En çok değer bunlarda, iş de en az.
  2. Sonra MCP.

### 2.3 Ham komut değil, anlam

- Ajanlar "şu tuşu şu renge boya" demez; bir anlam gönderir:
  - `waiting` (onay bekliyor), `working`, `success`, `error`, `attention`, `done`;
  - isteğe bağlı kısa bir metin.
- Her anlamın nasıl görüneceğini kullanıcı uygulamada seçer: renk, dalga, nabız, hangi tuşlar, ne kadar süre. Odak zamanlayıcısının ışık ayarları da böyle.
- Böylece bütün ajanlar aynı dili konuşur ve klavye kullanıcının zevkine göre kalır.
- Gelişmiş kullanım için sınırlı bir ham erişim olabilir (renk, dalga, kısa süre). Bu erişim de aynı katmandan ve aynı korumalardan geçer.

### 2.4 Işık ucuz, ekran pahalı

- **Tuş ışığı:** RAM'e gider (`04 20`, flash'a yazmaz), anında ve sınırsızdır. Sık sinyaller ışıkla verilir.
- **Ekran:** Her güncelleme flash'a yazar. Kart yaklaşık 2 sn, büyük animasyon 37 sn'ye kadar sürer ve günlük bütçeden düşer (DURUM maddeler 27, 32). Ekran yalnızca seyrek ve önemli mesajlar içindir: "uzun iş bitti", "onay gerekiyor".
  - Ajan başına bir sınır olur.
  - Aynı içerik ikinci kez yüklenmez.
  - Yükleme canlı mod bütçesinden düşer.
- **Ajanın kendi görseli:** Kullanıcının ilkesi burada da geçerli. Görsel önce klavyenin biçimine çevrilir ve önizlenir. Yalnızca kullanıcının izin verdiği ajanlar yükleyebilir.

### 2.5 Sıraya yeni bir kaynak

- Ajan sinyali, "Ekranda ne görünür?" sırasına (madde 31) ve ışık katmanına yeni bir kaynak olarak girer.
- Bitince ne olacağı baştan belli olmalı (DURUM ders 45): sinyal kısa sürer, sonra alttaki katmana döner. Bu alttaki katman zamanlayıcı, şarkı ya da kullanıcının kendi ışığı ve görseli olabilir.
- **Öncelik kararı gerekecek.** Örneğin "onay bekliyor" sinyali odak zamanlayıcısının üstünde mi olmalı? Muhtemel cevap: dikkat isteyen kısa sinyaller üstte durur, sonra önceki hâle dönülür.
- **Birden çok ajan:** Her sinyal kaynağının adıyla gelir (Claude Code, Codex…). Uygulamada son sinyallerin günlüğü görünür.

### 2.6 Kullanıcının kontrolü ve güvenlik

- Uygulamada **Araçlar → Ajanlar** sayfası olur. Kullanıcı buradan şunları yönetir:
  - aracın açık mı kapalı mı olduğu;
  - hangi ajanların bağlanabildiği;
  - hangi anlamın nasıl göründüğü;
  - ajanların ekrana yazıp yazamayacağı;
  - "rahatsız etme" saatleri;
  - son sinyallerin listesi.
- API'de firmware, bootloader, BLE OTA ya da flash'a doğrudan yazan komut **hiç olmaz**. Korumalar uygulamada kalır (`mimari.md` §7).
- Yerel API'ye yalnızca bu bilgisayardan erişilir ve bir anahtar gerekir.

### 2.7 Örnek senaryolar

- Claude Code izin istiyor: tuşlar sarı ve yavaş nefes alır. Onay verilince klavye kendi ışığına döner.
- Testler kırıldı: kırmızı tek dalga. Testler geçti: yeşil dalga.
- Uzun bir derleme bitti ve kullanıcı başka bir işte: ekrana "Derleme bitti · 4 dk" kartı gider. Bu seyrek olur ve bütçeden düşer.
- Ajan uzun bir göreve başlıyor: odak zamanlayıcısını başlatır. Ajan ve kullanıcı aynı süreyi görür.

## 3. Açık sorular

- **MCP mi, CLI mı?** Öneri ikisi de, aynı API üzerinden; kancalar CLI'ı kullanır. Kesin karar uygulama zamanında kullanıcıyla verilir.
- **Uygulama kapalıyken ne olur?** Tepside çalışan servis şart mı, yoksa araç uygulamayı kendisi mi başlatır?
- **Öncelikler:** Ajan sinyali ile odak zamanlayıcısı, şarkı ve LED canlı katmanı nasıl sıralanır?
- **Başka sistemler ve klavyeler:** macOS ve Linux sonra gelir. Diğer klavye modellerinde de (P4) aynı API çalışmalı. Anlam katmanı modelden bağımsız olduğu için bu mümkün görünüyor.
- **Ad:** Aracın ve komutların adları; Türkçe arayüzde Türkçe karşılıklar.
