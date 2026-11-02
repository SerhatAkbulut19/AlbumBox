# AlbumBox

Aile fotoğraf ve video arşivi. Harici diskteki dosyalar **yerinde** indekslenir. Kişi, yer, etkinlik ve tarih etiketleri SQLite'ta tutulur. Admin Panel'den yönetilir, aile üyeleri Client'tan yalnızca kendilerine yayınlananları görür.

Ayrıntılı tasarım ve evreler: [PLAN.md](PLAN.md)

## Klasörler
| Yol | İçerik |
|---|---|
| `crates/albumbox-core` | Domain, SQLite erişimi, migration'lar (`migrations/`), parola/oturum, görünürlük, iş kuyruğu |
| `crates/albumbox-indexer` | Disk bulma (`.albumbox`), tarama, hash, EXIF/ffprobe, küçük resim, arka plan motoru |
| `crates/albumbox-api` | axum REST API + SSE, statik UI sunumu, Windows Service, LAN koruması |
| `ui/` | Svelte 5 + TS arayüz. Metinler `ui/src/lib/i18n/tr.ts`, ikonlar `ui/src/lib/icons.ts` (Tabler Icons). `admin/` = Admin Panel, `client/` = Client |
| `scripts/ornek-disk.ps1` | Harici disk yokken test için örnek aile diski üretir ve sürücü harfine bağlar |

## Geliştirme
```powershell
# 1) Arayüzü derle (API ui/dist'i otomatik bulur)
cd ui; npm install; npm run build; cd ..

# 2) API'yi çalıştır -> http://localhost:8080
cargo run -p albumbox-api

# Arayüzde canlı geliştirme (API :8080 açıkken) -> http://localhost:5173
cd ui; npm run dev

# Testler
cargo test
cd ui; npm run check
```
İlk açılışta kullanıcı yoksa **Kurulum** ekranı gelir ve ilk yönetici hesabı oluşturulur.

### Harici disk olmadan deneme
```powershell
.\scripts\ornek-disk.ps1            # örnek klasörü üretir ve X: olarak bağlar
# Admin Panel > Kütüphaneler > Kütüphane ekle > X:\Aile

# Disk harfi değişimi: X:'i kaldırıp aynı klasörü Y: olarak bağla
subst X: /d; subst Y: "$env:TEMP\albumbox-ornek-disk"
# Birkaç saniye içinde kütüphane Y:\Aile olarak görünür.

.\scripts\ornek-disk.ps1 -Drive Y -Remove
```

## Nasıl çalışır (2. evre)
- **Kütüphane:** Seçilen kök klasöre gizli bir `.albumbox` dosyası yazılır. Diske yazılan tek şey budur. DB'de yalnızca disk kimliği ve köke göre göreli yol durur.
- **Disk bulma:** Motor `detect_interval_secs` aralıkla sürücüleri yoklar ve diski harfinden bağımsız olarak bulur. "Diskleri yeniden algıla" düğmesi bunu hemen yapar.
- **Tarama:** Artımlıdır. Boyutu ve değişiklik zamanı aynı olan dosyalar yeniden okunmaz. Yeni ya da değişen dosyaların içeriğinden blake3 özeti çıkarılır. Aynı içerik (kopya, taşınmış ya da adı değişmiş dosya) tek medya kaydına bağlanır; etiketler korunur. Diskten kaybolan dosyalar silinmez, "eksik" işaretlenir.
- **Tarih:** EXIF → video bilgisi (ffprobe) → dosya adı (`IMG_20220514_132210`, `VID-20230421-WA0012` …) → değişiklik zamanı.
- **Küçük resim:** JPEG 256 ve 1024 px, `data\cache\thumbs\` altında. Disk takılı değilken de gezinme bunlarla çalışır.
- **Motor:** Kendi thread'inde ve kendi runtime'ında çalışır. Admin Panel ilerlemeyi SSE ile canlı görür. Sunucu kapanırken yarım kalan iş, açılışta kaldığı yerden devam eder.

## Ayarlar: `albumbox.toml` (exe'nin yanında, ilk çalıştırmada oluşur)
| Anahtar | Varsayılan | Açıklama |
|---|---|---|
| `bind` | `0.0.0.0:8080` | Dinlenen adres |
| `data_dir` | `data` | DB, önbellek, loglar (exe klasörüne göre) |
| `ui_dir` | `ui` | Derlenmiş arayüz (`index.html` içeren klasör) |
| `lan_only` | `true` | Yerel ağ dışından gelen istekleri reddet |
| `session_hours` | `12` | "Beni hatırla" olmadan oturum süresi |
| `remember_days` | `30` | "Beni hatırla" ile oturum süresi |
| `log_level` | `info` | Log seviyesi |
| `ffmpeg_dir` | `""` | `ffmpeg.exe` + `ffprobe.exe` klasörü. Boşsa önce `<exe>\ffmpeg\`, sonra PATH kullanılır |
| `scan_workers` | `2` | Taramada paralel okunan dosya sayısı (USB disk için 2 iyi) |
| `thumb_workers` | çekirdek/2 (1-4) | Paralel küçük resim üretimi |
| `detect_interval_secs` | `10` | Disk takıldı/çıkarıldı kontrol aralığı |

Loglar `data\logs\albumbox-api.log.<tarih>` dosyalarına yazılır.

## Mini PC'ye kurulum (Windows Server)
1. `cargo build --release` ve `cd ui; npm run build`.
2. Hedef klasöre kopyala, ör. `C:\AlbumBox\`:
   - `target\release\albumbox-api.exe`
   - `ui\dist\*` dosyaları `C:\AlbumBox\ui\` altına
   - ffmpeg'in `bin` klasöründeki `ffmpeg.exe` ve `ffprobe.exe` dosyaları `C:\AlbumBox\ffmpeg\` altına. Servis LocalSystem hesabıyla çalışır ve kullanıcının PATH'ini görmez.
3. Yönetici PowerShell'de: `C:\AlbumBox\albumbox-api.exe service install`. Servis `AlbumBoxService` adıyla otomatik başlar.
4. Güvenlik duvarında 8080/TCP portunu yalnızca yerel ağa aç.
5. Kaldırmak için: `albumbox-api.exe service uninstall`.

Şifre unutulursa (servis durdurulmuşken): `albumbox-api.exe reset-password <kullanıcı> <yeni-şifre>`

## API özeti
| Uç | Yetki |
|---|---|
| `GET /api/health` | herkes |
| `GET/POST /api/setup` | herkes (POST yalnızca hiç kullanıcı yokken) |
| `POST /api/auth/login`, `POST /api/auth/logout`, `GET /api/auth/me` | — / oturum |
| `GET /api/home` (albümler, yıllar, son eklenenler) | oturum (görünürlük filtresiyle) |
| `GET /api/search?q=&tags=&year=&category=&group=event\|year` (gruplar + facet'ler) | oturum |
| `GET /api/search/media?…&key=<grup>&offset=` (bir grubun medyası) | oturum |
| `GET /api/albums`, `GET /api/albums/{id}/media`, `GET /api/media/{id}` | oturum |
| `GET /api/media/{id}/thumb/{256\|1024}`, `GET /api/media/{id}/original` (Range destekli) | oturum. Görülemeyen medya için 404, disk takılı değilse orijinal için 409 |
| `GET /api/admin/stats`, `GET/POST /api/admin/users`, `PUT /api/admin/users/{id}`, `PUT /api/admin/users/{id}/password` | admin |
| `GET/POST /api/admin/libraries`, `PUT /api/admin/libraries/{id}`, `POST /api/admin/libraries/{id}/scan`, `POST /api/admin/libraries/refresh` | admin |
| `GET /api/admin/libraries/{id}/folders`, `POST /api/admin/libraries/{id}/folders/tag` | admin |
| `GET /api/admin/fs?path=` (sunucu klasör gezgini) | admin |
| `GET /api/admin/engine`, `GET /api/admin/events` (SSE), `GET /api/admin/jobs`, `POST /api/admin/jobs/{id}/cancel` | admin |
| `GET /api/admin/media` (filtre + `offset`/`limit`), `GET /api/admin/media/ids` (filtredeki tümü), `GET /api/admin/media/{id}`, `POST /api/admin/media/bulk` | admin |
| `GET/POST /api/admin/categories`, `PUT/DELETE /api/admin/categories/{id}`, `PUT …/{id}/move`, `PUT …/{id}/shares`, `PUT …/{id}/cover` | admin |
| `GET/POST /api/admin/tag-types`, `PUT/DELETE /api/admin/tag-types/{id}` | admin |
| `GET /api/admin/tags` (`?type=`, `?q=` öneri), `POST /api/admin/tags` (varsa mevcut olanı döner), `PUT/DELETE /api/admin/tags/{id}`, `POST …/{id}/merge` | admin |
| `GET/POST /api/admin/profiles`, `PUT/DELETE /api/admin/profiles/{id}`, `PUT …/{id}/members` | admin |

**Medya filtreleri:** `q`, `tags` (virgülle id, hepsi), `category`, `uncategorized`, `untagged`, `library` + `folder`, `kind`, `year`, `visibility` (`private`/`published`/`hidden`/`all`), `profile`, `sort` (`taken_desc`/`taken_asc`/`added_desc`).

**Toplu işlemler** (`action`): `add_tags`, `remove_tags`, `add_to_category` (kopyala), `move_to_category` (taşı), `remove_from_category`, `set_visibility` (`inherit`/`private`/`profiles`), `set_hidden`, `set_taken_at`. Görünürlüğü etkileyen her işlem `media_access`'i aynı transaction içinde yeniden hesaplar.

## Arama nasıl çalışır (4. evre)
- Kelimeler boşluk ya da virgülle ayrılır ve **hepsi** eşleşmelidir: `köy, nişan` → hem Köy hem Nişan.
- Bir kelime şunlardan biri tutarsa eşleşir:
  - Bir etiketin herhangi bir kelimesinin başı. Türkçe karakterden bağımsızdır: "koy" = "Köy".
  - Bir üst etiket: "Sivas" araması, Sivas'ın altındaki "Köy" etiketli fotoğrafları da getirir.
  - Klasör adındaki bir kelime.
  - Yıl ("2022").
- Sonuçlar varsayılan olarak **etkinlik etiketlerine** göre gruplanır; etkinliği olmayanlar "Etkinliksiz" grubundadır. İstenirse **yıla** göre gruplanır.
- Yan paneldeki Kişi, Yer, Etkinlik ve Yıl seçenekleri sayılarla gelir; tıklanınca arama daralır.
- Aile üyeleri yalnızca kendilerine yayınlanan medyayı ve o medyanın etiketlerini görür. Özel fotoğraftaki kişi ya da etiket aramada hiç görünmez.
- Albümler: üst albüm, alt albümlerinin fotoğraflarını da sayar ve kapağını onlardan alır. Özel bir üst albümün altındaki yayınlanmış albüm en üst seviyede görünür, böylece özel albümün adı açığa çıkmaz.

## Yayın nasıl çalışır (3. evre)
- Yeni medya ve yeni kategori **özel** başlar.
- Kategori profillere yayınlanır. İçindeki medya **"Kategoriden al"** ayarındaysa o profillere açılır.
- Tek bir medya **Özel** ya da **Seçilen profiller** yapılabilir. Bu ayar kategorinin yayınını geçersiz kılar.
- **Gizle** her şeyin önündedir: gizli medyayı yöneticiler dışında kimse görmez. Admin listesinde de varsayılan olarak gizlidir.
- Profil üyeliği değişince yeniden hesap gerekmez, çünkü erişim profil bazında tutulur.
- **Taşı / Kopyala** yalnızca kategori üyeliğini değiştirir; diskteki dosyalara dokunmaz.

Kimlik doğrulama: web'de HttpOnly `ab_session` cookie, masaüstünde `Authorization: Bearer <token>`.
