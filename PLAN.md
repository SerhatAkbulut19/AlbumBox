# AlbumBox: Aile Fotoğraf ve Video Arşivi

## Bağlam
Harici diskte karışık klasörlerde aile fotoğrafları ve videoları var. Hedef şunlar:
- Dosyalara dokunmadan bu diski **indekslemek**.
- Her dosyaya **Kişi / Yer / Etkinlik / Tarih / Serbest** etiketleri vermek ve bunları DB'de tutmak.
- Panelden aramak. Örneğin "Köy" yazınca o yerdeki etkinlikler (Nişan 2022, Ramazan Bayramı 2023 …) gruplanmış gelsin, tıklayınca fotoğraflar açılsın.
- Kişileri baştan **yüz tanıma** ile bulmak.
- 2. evre: müzikli slayt gösterisi.

### Onaylanan kararlar
| Konu | Karar |
|---|---|
| Ad | **AlbumBox** (kod ve proje adı İngilizce, arayüz metinleri Türkçe, açıklamalar ve dokümanlar Türkçe) |
| Konum | `Desktop\Claude\AlbumBox`. Geliştirme başka PC'de yapılır, hedef Windows Server mini PC'dir. En başta `BeniOku.md` + `PLAN.md` yazılır. |
| API | Rust (axum + tokio), mini PC'de Windows Service: `AlbumBoxService` |
| DB | SQLite (WAL + FTS5), API'ye gömülü: `albumbox.db` |
| Dosyalar | **Yerinde kalır, sadece indekslenir.** Hiçbir dosya kopyalanmaz, taşınmaz, değiştirilmez. |
| Disk | Sürücü harfi değişebilir, klasör ağacı aynı kalır. Yollar **disk kimliği + köke göre göreli yol** olarak tutulur. |
| Arayüz | Tek Svelte 5 + TS kod tabanı (WASM yok). API bunu web paneli olarak sunar, Tauri 2 de `AlbumBox.exe` masaüstü programı yapar. |
| Yüz tanıma | 1. evrede, yerel ONNX ile |
| Erişim | Sadece ev ağı. **Login ekranı + kullanıcı hesapları** (2026-11-02 güncellemesi, tek parola yerine). |
| Arayüz grupları | **Admin Panel** (yönetim) + **Client** (izleme). Tek Svelte uygulamasında iki bölüm: `/admin/*` ve `/`. Yetki sunucuda denetlenir. |
| Yayın | Kategori ve resim bazında yayın. Varsayılan **özel** (yalnız admin görür). Seçilen profillere ya da tüm aileye yayınlanabilir. Client yalnızca giriş yapan kimliğin görebildiği veriyi çeker. |
| Taşıma/aktarma | Mantıksaldır: kategoriler arası taşıma ve kopyalama. **Diskteki dosyalara dokunulmaz.** |

## Disk kimliği ve göreli yol
- Diskin kök klasörüne (ya da kullanıcının seçtiği kütüphane köküne) ilk taramada `.albumbox` işaret dosyası yazılır. İçinde `library_id` (UUID) ve ad ("Aile Diski") bulunur. **Bu dosya diske yazılan tek şeydir.**
- API açılışta ve periyodik olarak (ayrıca Windows cihaz takma olayında) tüm sürücü harflerini tarar ve `.albumbox` bulduğu kökü `library_id`'ye eşler. Harf E: iken F: olsa da eşleşme kendiliğinden düzelir.
- İşaret dosyası yazılamazsa yedek kimlik olarak volume serial/GUID kullanılır.
- DB'de yalnızca göreli yol durur, örneğin `Aile\2022 Nişan\IMG_0412.JPG`. Tam yol çalışma anında `<mevcut kök harfi> + göreli yol` olarak üretilir.
- **Disk takılı değilken**: arama, etiketleme, gezinme ve küçük resimler çalışır (önbellek mini PC'de). Yalnızca orijinali açma ve video oynatma "Disk takılı değil" uyarısı verir.

## Mimari
```
[Harici disk (.albumbox)] <--okuma--  [albumbox-api servisi (mini PC)]  --HTTP/SSE-->  [Web panel]
                                          |  axum, arka plan işçileri (ayrı thread)       [AlbumBox.exe (Tauri)]
                                          |-- data\albumbox.db (SQLite WAL)
                                          |-- data\cache\thumbs\  (JPEG 256 / 1024 px; videolarda kareden)
                                          |-- data\models\        (ONNX yüz modelleri)
```
UI ile iş thread'leri ayrı tutulur. Tarama, küçük resim üretimi ve yüz tanıma kuyruklu arka plan işçilerinde koşar. API hiç beklemez, durum anlık görüntüden okunur ve ilerleme SSE ile akar. `data\` yolu ayardan gelir.

### Cargo workspace
| Crate | Görev | Ana bağımlılıklar |
|---|---|---|
| `crates/albumbox-core` | Domain, DB, migration, arama sorguları, Türkçe normalizasyon | `sqlx` (sqlite), `serde`, `time` |
| `crates/albumbox-indexer` | Kütüphane kökü bulma, tarama, blake3 hash, EXIF/tarih, küçük resim, video posteri | `walkdir`, `blake3`, `kamadak-exif`, `image` + `fast_image_resize`, `webp`, ffmpeg/ffprobe (harici exe) |
| `crates/albumbox-faces` | SCRFD yüz tespiti, ArcFace 512 boyutlu embedding, kümeleme, öneri | `ort` (ONNX Runtime CPU), `ndarray` |
| `crates/albumbox-api` | REST + SSE, statik UI sunumu, parola/token, Windows Service, işçi havuzu | `axum`, `tower-http` (Range/static/gzip), `windows-service`, `tracing` |
| `ui/` | Svelte 5 + Vite + TS. Metinler `ui/src/lib/i18n/tr.ts` dosyasında Türkçe tutulur. | `svelte` |
| `desktop/` | Tauri 2 kabuğu, `ui/` derlemesini yükler, API adresi ayardan gelir | `tauri` |

## Veritabanı şeması (İngilizce adlar)
- `libraries`: id (UUID = `.albumbox` içindeki), name, last_seen_root (ör. `F:\`), online, last_scan_at.
- `media`: içerik kaydıdır, **hash başına bir satır** tutulur. Alanlar: id, hash (blake3, unique), kind (photo/video), width, height, duration_ms, taken_at, taken_at_source (exif/filename/mtime/manual), gps_lat, gps_lon, camera, thumb_ready.
- `media_files`: diskteki konumlardır. Aynı içerik birden çok klasördeyse birden çok satır olur. Alanlar: id, media_id, library_id, rel_path, folder_rel, size, mtime, missing.
- `tag_types`: person, place, event, date, custom.
- `tags`: id, type_id, name (görünen hâli, ör. "Nişan 2022"), name_norm (Türkçe katlanmış: İ→i, I→ı, ç→c …), parent_id (ör. Köy → Sivas).
- `media_tags`: media_id, tag_id, source (manual / face / folder_suggestion).
- `persons`: tag_id, cover_face_id, centroid BLOB.
- `faces`: id, media_id, bbox, embedding BLOB (512×f32), person_id, cluster_id, score, status (suggested/confirmed/rejected).
- `tags_fts` (FTS5, prefix arama) ve `jobs` (tarama ve işçi kuyruğu, kaldığı yerden devam eder).
- `users`: id, username, display_name, password_hash (Argon2id), role (admin/member), is_active, created_at, last_login_at.
- `sessions`: token_hash (SHA-256), user_id, created_at, expires_at, user_agent. Web için HttpOnly cookie, Tauri için Bearer.
- `profiles`: yayın kitleleridir (ör. "Tüm Aile", "Yakın Aile", "Çocuklar"). Alanlar: id, name, is_builtin. "Tüm Aile" yerleşiktir, her kullanıcı otomatik üyedir.
- `user_profiles`: user_id, profile_id.
- `categories`: albüm ağacıdır (ör. Bayramlar → Ramazan 2023). Alanlar: id, parent_id, name, description, cover_media_id, sort_order.
- `media_categories`: media_id, category_id. Bir resim birden çok kategoride olabilir.
- `category_shares` (category_id, profile_id) ve `media_shares` (media_id, profile_id). Paylaşım yoksa kayıt **özel** sayılır.
- `media.visibility_mode`: `inherit` (varsayılan; kategorilerinden alır) / `custom` (kendi `media_shares` kaydı geçerli, boşsa özel).
- `media_access` (media_id, profile_id): hesaplanmış görünürlük tablosudur. Yayın, kategori ya da üyelik değişince ilgili medyalar için yeniden hesaplanır. Client sorguları buraya join eder (`profile_id IN kullanıcının profilleri`), böylece filtre hızlı kalır.

### Görünürlük kuralı
- **Admin** her şeyi görür.
- **Member** bir medyayı ancak `media_access` içinde kendi profillerinden biri varsa görür:
  - `inherit` modunda medya, kategorilerinin yayınlandığı profillerin birleşimine açılır.
  - `custom` modunda medya yalnızca kendi `media_shares` profillerine açılır, kategoriden miras almaz. Bu yolla bir resim, kategorisi yayında olsa bile gizlenebilir.
- Kategori, member'a ancak içinde görebildiği medya varsa ya da kategorinin kendisi paylaşılmışsa listelenir.
- Etiket, kişi ve etkinlik facet'leri yalnızca görünür medyalardan sayılır. Gizli bir resimdeki kişi ya da etiket client'ta hiç görünmez.
- Küçük resim, orijinal ve video uçları da aynı denetimden geçer. ID tahmin edilerek gizli resme erişilemez.

Etiketler `media`'ya bağlıdır, dosyaya değil. Böylece aynı fotoğrafın kopyaları da etiketi paylaşır.

**Gruplu arama** (`GET /api/search?q=köy`):
1. FTS5 ile eşleşen etiketler bulunur (Köy · yer).
2. Bu etiketi taşıyan medyalarda birlikte geçen **etkinlik** etiketleri sayılarıyla gruplanır: Nişan 2022 (148), Ramazan Bayramı 2023 (63).
3. Kişiler ve yıllar yan facet olarak gelir.
4. Birden çok terim AND ile çalışır: "köy ahmet 2022".

## Tarama akışı
1. Panelde "Kütüphane ekle" ile kök klasör seçilir. `.albumbox` yazılır ve `libraries` kaydı oluşur.
2. Tarama: `walkdir` ile dosya listesi çıkarılır. Daha önce görülen dosyalar (rel_path + size + mtime aynıysa) atlanır, yalnızca yeni ve değişen dosyalar işlenir.
3. Yeni dosya için blake3 hash hesaplanır.
   - Hash `media`'da zaten varsa yalnızca `media_files`'a yeni konum eklenir (kopya dosya).
   - Bir dosya taşınmış ya da yeniden adlandırılmışsa hash aynı kalır, eski konum `missing` olur, yenisi eklenir. Böylece etiketler korunur.
4. Tarih sırası: EXIF DateTimeOriginal → video `creation_time` (ffprobe) → dosya adı kalıpları (`IMG_20220514_…`, `VID-20230421-WA…`) → mtime.
5. **Klasör adı önerisi**: "Ramazan tatili", "Düğün Ayşe" gibi klasör adları etkinlik etiketi önerisi olur, kullanıcı tek tıkla onaylar ya da düzeltir.
6. Küçük resim ve video posteri mini PC önbelleğine üretilir. Ardından yüz işi kuyruğa girer.
- Desteklenenler: JPG/PNG/WebP/GIF/BMP/TIFF Rust ile, HEIC ve diğerleri ffmpeg ile çözülür. Videolar: MP4/MOV/3GP/AVI/MKV/MTS … RAW şimdilik dışarıda.

## Yüz tanıma
- SCRFD (tespit) + ArcFace (embedding) ONNX ile CPU'da tamamen yerelde çalışır. İnternet gerekmez. (InsightFace modelleri ticari olmayan kullanım içindir, ev kullanımı için uygun.)
- Embedding'ler DB'de durur ve bellekte tutulur. 100 bin yüz yaklaşık 200 MB eder, kaba kuvvet kosinüs araması yeterli.
- Kümeleme: isimsiz yüzler eşik tabanlı kümelenir. "Kişiler" sayfasında kümeye "Ahmet" denince tüm yüzler etiketlenir.
- Öneri: yeni yüz, bir kişinin merkezine yakınsa "Ahmet olabilir mi?" diye sorulur. Onay ya da red sonrası merkez güncellenir.
- Videolardan birkaç kare örneklenir ve aynı akıştan geçer.

## Arayüz (Türkçe metinler, kompakt, soft renk, tema)

### Login ve ilk kurulum
- Ortak **Login ekranı**: kullanıcı adı + şifre, "beni hatırla" seçeneği.
  - Admin girişte Admin Panel'e yönlenir; isterse üst menüden Client'a geçer.
  - Member yalnızca Client'a girer.
- İlk çalıştırmada kullanıcı yoksa **Kurulum ekranı** açılır ve ilk admin hesabı oluşturulur.
- Hatalı girişe hız sınırı uygulanır (IP başına deneme kısıtı).

### Admin Panel (`/admin`)
- **Gösterge**: medya, kişi ve kategori sayıları, son taramalar, bekleyen yüz önerileri, özel/yayında oranı.
- **Medya**: tüm arşiv sanal ızgarada gösterilir, facet ve arama filtresi vardır.
  - **Toplu seçim**: tıklama, Shift ile aralık, Ctrl ile tekli, sürükleyerek kutu seçimi ya da "filtredekilerin tümünü seç".
  - Toplu işlemler: etiket ekle/çıkar, kategoriye **taşı** / **kopyala** (aktar) / kategoriden çıkar, yayın ayarla (miras / özel / profiller), tarih düzelt, kişi ata, arşivden gizle.
- **Kategoriler**: sürükle-bırak ağaç, kapak seçimi, sıralama, yayın ayarı (profil seçimi), alt kategorilere uygula.
- **Etiketler**: etiket tipleri (Kişi, Yer, Etkinlik, Serbest + yenileri). Etiketlerde yeniden adlandırma, birleştirme ve üst etiket (Köy → Sivas) yapılır.
- **Kişiler**: yüz kümeleri, isim verme, birleştirme/ayırma, öneri onayı.
- **Kullanıcılar**: ekleme, düzenleme, pasifleştirme, şifre sıfırlama, rol ve profil üyelikleri.
- **Profiller (yayın kitleleri)**: oluşturma ve üye yönetimi, "bu profil ne görüyor" önizlemesi.
- **Kütüphaneler**: disk durumu (çevrim içi, harf, son tarama), tarama başlatma, canlı ilerleme, klasör adı önerileri (etkinlik/kategori olarak onayla).

### Client (`/`)
Giriş yapan kimliğin görebildiği veri gösterilir.
- **Ana sayfa**: görünür kategoriler kapaklarıyla ve son eklenenler.
- **Ara**: gruplu arama ("Köy" → etkinlik grupları), facetler (Etkinlik, Kişi, Yer, Yıl), ızgara ve liste görünümü.
- **Kategori / Kişi sayfaları**: alt kategoriler ve medya.
- **Görüntüleyici**: tam ekran, ←/→, yüz isimleri, video (HTTP Range). Disk takılı değilse 1024 px önbellek gösterilir.
- Client'ta düzenleme yoktur. (İleride "favorile" eklenebilir.)

### Yetki
- `/api/admin/*` uçları yalnızca admin rolüne açıktır.
- `/api/*` client uçları her sorguda `media_access` filtresi uygular.
- API yalnızca yerel ağı dinler.

## Evreler
1. ✅ **İskelet + kimlik** (2026-11-02 tamamlandı; profil yönetimi 3. evrede): workspace, tüm migration, axum, ayar dosyası, Windows Service, kullanıcı/oturum/rol, kurulum ve login ekranı, Svelte iskeleti (Admin ve Client yerleşimleri, route koruması), API'nin UI'yi sunması.
2. ✅ **Kütüphane + tarama** (2026-11-02 tamamlandı; harici disk `subst` ile simüle edildi, mini PC'de doğrulanacak): `.albumbox`, sürücü eşleme, artımlı tarama, hash, tarih, küçük resim/poster, SSE ilerleme. Admin Kütüphaneler sayfası.
3. ✅ **Yönetim + yayın** (2026-11-02 tamamlandı): kategoriler, etiketler, profiller, kullanıcılar; Admin Medya sayfası ve toplu seçim/işlemler; `media_access` hesaplaması.
4. ✅ **Client + arama** (2026-11-02 tamamlandı): FTS5 gruplu arama, facetler, kategori sayfaları, görüntüleyici; görünürlük filtresi testleri.
5. **Yüz tanıma**: ONNX işçisi, kümeleme, Kişiler sayfası, öneriler.
6. **Tauri paketi**: `AlbumBox.exe`, API adresi ayarı.
7. **2. evre**: müzikli slayt, GPS harita, benzer foto tespiti, video dönüştürme.

## Doğrulama
- `cargo test`: görünürlük kuralları (miras, özel, profil, gizli resim + yayında kategori, member'ın gizli ID ile thumb isteği → 404), login/oturum, tarih çıkarma, Türkçe normalize, hash ile kopya/taşıma tespiti, sürücü harfi değişince yeniden eşleme, gruplu arama SQL'i.
- Uçtan uca örnek kütüphane: 50 karışık dosya, kopyalar ve Türkçe klasör adları.
  1. Taranır.
  2. Klasör yeniden adlandırılır ve tekrar taranır: etiketlerin korunduğu kontrol edilir.
  3. Disk/klasör başka harfe bağlanır (`subst`): otomatik eşleme kontrol edilir.
  4. Disk çıkarılmışken arama ve küçük resimlerin çalıştığı kontrol edilir.
- Yüz: bilinen 3-4 kişili fotoğraflarla küme ve öneri doğruluğu kontrol edilir.
- Web panel Playwright ile test edilir (önce ekran DPI'ı ölçülür): "Köy" araması, gruplar, toplu etiketleme, görüntüleyici. Tauri `cargo tauri dev` ile aynı akıştan geçer.
- Release ya da kurulum paketinden önce kullanıcıya sorulur. Git işlemi yapılmaz.

## Ön koşullar (uygulama başında kontrol)
- Geliştirme PC'si: Rust stable, Node 20+, ffmpeg/ffprobe.
- Mini PC: ffmpeg ve `onnxruntime.dll` dağıtım klasörüne konur, `data\` için yer ayrılır.
