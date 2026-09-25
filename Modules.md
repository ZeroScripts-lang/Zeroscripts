# ZeroScript — Module System

46. Module / Import

ZeroScript menggunakan konsep module dengan syntax ESM-like.

import { tambah } from "./math"

Ekstensi ".zs" tidak perlu ditulis.

import { tambah } from "./math"

akan mencari module:

./math.zs

Tidak ada "require()".

---

47. Export

Named export:

export function tambah(A, B) {
    return A + B
}

export const VERSION = "1.0"

Atau:

function tambah(A, B) {
    return A + B
}

const VERSION = "1.0"

export { tambah, VERSION }

Import:

import { tambah, VERSION } from "./math"

Default export belum menjadi bagian dari spesifikasi saat ini.

---

61. Module Initialization

Setiap file .zs yang di-import dianggap sebagai module dengan scope-nya sendiri.

Contoh:

// utils.zs
let A = 10

export function getA() {
    return A
}

// main.zs
import { getA } from "./utils"

print(getA()) // 10

Aturannya:

Module dieksekusi ketika pertama kali di-import.

Module yang sama hanya diinisialisasi satu kali dalam satu eksekusi program.

Import berikutnya menggunakan module yang sudah diinisialisasi.

Variable/function yang tidak di-export tetap private di dalam module.

Importer hanya dapat mengakses sesuatu yang memang di-export.

Module memiliki scope sendiri dan tidak berbagi variable global secara langsung dengan module lain.

Urutan inisialisasi mengikuti dependency:

main.zs import A

A import B

maka B diinisialisasi terlebih dahulu, lalu A, lalu main.



Contoh:

// a.zs
print("A")

export const X = 10

// main.zs
import { X } from "./a"
import { X } from "./a"

print(X)

Output:

A
10

Import kedua tidak menjalankan ulang a.zs.

--- -----

62. Circular Import
Kita tentukan perilaku kalau module saling meng-import:
A → B
B → A

Contoh:
// a.zs
import { B } from "./b"
export const A = 10
// b.zs
import { A } from "./a"
export const B = 20
Saat runtime mendeteksi dependency cycle:
ERROR: Circular module dependency detected
Aturan:
Circular dependency langsung maupun tidak langsung → ERROR.
Error terjadi saat initialization.
Tidak ada module yang boleh mendapat nilai/setengah-inisialisasi module lain.
Tidak ada perilaku khusus untuk “memecahkan” cycle secara otomatis.
Jadi dependency graph ZeroScript harus berbentuk acyclic.

--- ---- ---

63. Module Import/Export Errors

ZeroScript harus gagal secara eksplisit kalau module atau export yang diminta tidak tersedia.

Module tidak ditemukan

import { tambah } from "./math"

Jika ./math.zs tidak ada:

ERROR: Module not found

Export tidak ditemukan

Misalnya math.zs hanya punya:

export function tambah(A, B) {
    return A + B
}

Tetapi:

import { kurang } from "./math"

Maka:

ERROR: Export 'kurang' not found

Aturan

Module path tidak ditemukan → ERROR.

Module gagal di-load/initialize → ERROR.

Named export tidak ditemukan → ERROR.

Import tidak boleh diam-diam menghasilkan null.

Tidak ada fallback otomatis.

Nama export harus cocok secara case-sensitive.

Export yang tidak digunakan tetap boleh ada.


Contoh:

import { Tambah } from "./math"

berbeda dengan:

import { tambah } from "./math"

Kalau yang tersedia hanya tambah, Tambah → ERROR.

--- --- ---

64. Module Path Resolution

Import menggunakan path relatif terhadap file yang melakukan import.

import { tambah } from "./utils"

Jika file:

project/
├── main.zs
└── utils.zs

maka ./utils → utils.zs.

Subdirectory:

import { auth } from "./lib/auth"

→ lib/auth.zs.

Aturan:

Extension .zs otomatis ditambahkan.

./ berarti direktori file saat ini.

../ berarti satu tingkat ke atas.

Absolute path boleh didukung runtime, tetapi bukan bentuk utama module import.

Path harus menunjuk ke file .zs.

Directory import seperti ./utils/ → ERROR jika tidak menunjuk ke module yang valid.

Jika path setelah resolusi tidak ditemukan → ERROR.

Path resolution tidak bergantung pada current working directory; acuannya adalah lokasi file yang melakukan import.


Jadi kalau:

project/
├── main.zs
├── lib/
│   ├── math.zs
│   └── utils/
│       └── text.zs

main.zs:

import { tambah } from "./lib/math"

dan math.zs:

import { trim } from "./utils/text"

keduanya resolve berdasarkan lokasi masing-masing file.

--- --- ---

65. Module Scope & Encapsulation
Setiap module memiliki scope terpisah.
// math.zs
let A = 10

export function getA() {
    return A
}
// main.zs
import { getA } from "./math"

print(getA()) // 10
print(A)      // ERROR
Aturan:
Variable/function yang tidak di-export hanya dapat digunakan di dalam module tersebut.
export membuat binding tersedia untuk module lain.
Import tidak membuat seluruh scope module sumber menjadi terbuka.
Module tidak dapat mengakses variable private module lain secara langsung.
Dua module boleh memiliki variable dengan nama sama tanpa konflik.
Contoh:
// a.zs
let value = 10
export function get() {
    return value
}
// b.zs
let value = 20
export function get() {
    return value
}
Keduanya valid karena value berada di scope module masing-masing.

--- --- ---

66. Module Export Binding

Sekarang kita kunci perilaku nilai yang di-export.

ZeroScript menggunakan live binding, bukan snapshot.

// counter.zs
export let count = 0

export function increment() {
    count++
}

// main.zs
import { count, increment } from "./counter"

print(count)      // 0
increment()
print(count)      // 1

Aturannya:

export let mempertahankan binding ke variable asli.

Perubahan pada variable tersebut terlihat oleh module yang meng-import.

export const tidak dapat diubah setelah deklarasi.

Module lain tidak boleh melakukan assignment langsung terhadap imported binding.


Contoh:

import { count } from "./counter"

count = 10 // ERROR

Kalau ingin mengubah state module, gunakan function yang memang disediakan module tersebut.

Ini mengikuti konsep module yang eksplisit: import memberi akses ke binding, bukan kepemilikan atas variable tersebut.

Gas 67. Module Re-export

Biar module bisa dijadikan facade/barrel, kita izinkan re-export.

// math.zs
export function tambah(A, B) {
    return A + B
}

// utils.zs
export { tambah } from "./math"

Lalu:

// main.zs
import { tambah } from "./utils"

print(tambah(2, 3))

Aturan

Module boleh me-re-export named export dari module lain.

Re-export tidak membuat salinan function/value.

Binding tetap mengacu pada export asli.

Jika source module atau export yang dire-export tidak ditemukan → ERROR.

Duplicate export dalam module yang sama → ERROR.


Jadi bisa bikin struktur:

lib/
├── math.zs
├── string.zs
└── index.zs

index.zs dapat menjadi pintu masuk:

export { tambah } from "./math"
export { upper } from "./string"

Lalu pengguna cukup:

import { tambah, upper } from "./lib/index"

Simple dan berguna buat library.

--- ----|

68. Module Export Conflict

Kita kunci konflik nama export.

Misalnya satu module melakukan:

export { tambah } from "./math"
export { tambah } from "./other"

Ini → ERROR, karena module tersebut mencoba menyediakan dua export dengan nama tambah.

Begitu juga:

export const A = 10
export { A } from "./other"

→ ERROR.

Aturan:

Setiap module hanya boleh memiliki satu binding untuk setiap nama export.

Duplicate export → ERROR.

Konflik terdeteksi saat module dianalisis/diinisialisasi.

Tidak ada export yang otomatis menimpa export lain.

Nama export bersifat case-sensitive.


Dengan ini struktur export/re-export kita nggak punya perilaku ambigu.

--- --- ---

69. Module Import Alias

Import named export boleh diberi nama lokal menggunakan as.

import { tambah as add } from "./math"

print(add(2, 3))

Aturan:

Alias hanya mengubah nama lokal hasil import.

Nama export asli tidak berubah.

Alias bersifat case-sensitive.

Dua import boleh mengarah ke export berbeda selama nama lokalnya tidak konflik.

Jika alias bentrok dengan deklarasi lokal pada scope yang sama → ERROR.

Alias tidak memengaruhi module sumber.


Contoh:

import { tambah as add } from "./math"
import { kurang as sub } from "./math"

print(add(10, 5))
print(sub(10, 5))

Valid.


---

70. Module Import Multiple Bindings

Satu statement import boleh mengambil beberapa named export dari module yang sama:

import {
    tambah,
    kurang,
    kali
} from "./math"

Aturan:

Semua nama yang disebut harus merupakan export yang valid.

Setiap nama dapat menggunakan alias.

Duplicate local binding → ERROR.


Contoh:

import {
    tambah as add,
    kurang as sub,
    kali
} from "./math"

Valid.

Tidak perlu syntax tambahan seperti import *, require(), atau bentuk wildcard lainnya untuk saat ini.

--- ----

71. Module Import Initialization Order
Kita kunci urutan ketika satu module punya beberapa dependency:
// main.zs
import { A } from "./a"
import { B } from "./b"
Jika a.zs dan b.zs tidak saling bergantung, initialization mengikuti urutan dependency yang muncul di source:
main
 ├─ a
 └─ b
Maka:
a → b → main
Aturan:
Dependency module diinisialisasi sebelum module yang mengimpornya.
Jika beberapa dependency tidak saling bergantung, urutannya mengikuti urutan import dalam source.
Setiap module hanya diinisialisasi sekali.
Circular dependency tetap ERROR sesuai Section 62.
Setelah seluruh dependency selesai, barulah kode module importer dijalankan.
Jadi initialization order ZeroScript deterministik, bukan bergantung pada urutan filesystem atau runtime yang kebetulan terjadi.

--- ----

72. Module Import Execution
Kita bedakan import dengan execution secara jelas.
Saat module di-import:
ZeroScript resolve path module.
Mengecek dependency module tersebut.
Dependency diinisialisasi terlebih dahulu.
Module diinisialisasi satu kali.
Setelah selesai, export module tersedia untuk importer.
Import berikutnya memakai module yang sudah diinisialisasi.
Contoh:
// counter.zs
let count = 0

count++

export function get() {
    return count
}
// main.zs
import { get } from "./counter"
import { get as get2 } from "./counter"

print(get())  // 1
print(get2()) // 1
counter.zs tidak menjalankan count++ dua kali.
Jadi sederhananya:
Import pertama → module dijalankan.
Import berikutnya → module yang sama digunakan kembali.

---

73. Module Evaluation Failure
Kalau kode saat initialization module menghasilkan runtime error, proses import gagal.
Contoh:
// config.zs
let A = 10 / 0

export const VALUE = A
Lalu:
// main.zs
import { VALUE } from "./config"
Maka import tersebut menghasilkan runtime error.
Aturan:
Error saat module initialization → module dianggap gagal diinisialisasi.
Export dari module yang gagal tidak tersedia.
Error dapat ditangkap oleh try/catch jika terjadi pada operasi import yang memang dieksekusi runtime.
Module yang gagal tidak dianggap berhasil di-cache.
Import berikutnya akan mencoba initialization kembali jika eksekusi berlanjut dan kembali melakukan import.
Tujuannya supaya ZeroScript tidak punya kondisi aneh seperti module setengah berhasil tetapi sebagian export sudah dianggap valid.

---

74. Module Export Mutability
Kita kunci bagaimana let dan const yang di-export berperilaku.
// counter.zs
export let count = 0
export const MAX = 100

export function increment() {
    count++
}
Aturan:
export let tetap mutable di dalam module asal.
export const tetap immutable.
Module yang meng-import tidak boleh melakukan assignment terhadap imported binding.
Perubahan export let dari module asal terlihat oleh importer karena menggunakan live binding.
Re-export mempertahankan aturan mutability dari binding asli.
Contoh:
import { count, increment } from "./counter"

increment()

print(count) // 1

count = 10 // ERROR
Jadi importer bisa membaca binding dan melihat perubahan state, tetapi bukan mengambil alih hak untuk mengubahnya.

---

75. Module Import Declaration Scope
import hanya berlaku pada scope module tempat import tersebut ditulis.
Contoh:
// main.zs
import { tambah } from "./math"

function test() {
    print(tambah(1, 2))
}
Valid, karena test() berada dalam module yang sama.
Tapi module lain tidak otomatis mendapatkan tambah:
// other.zs
print(tambah(1, 2)) // ERROR
Aturan:
Imported binding tersedia di seluruh module tempat import dideklarasikan.
Import tidak otomatis menjadi global.
Import tidak otomatis diwariskan ke module lain.
Module lain harus melakukan import sendiri.
Imported binding mengikuti aturan scope biasa.
Nama import yang konflik dengan deklarasi pada scope yang sama → ERROR.

--- ----

76. Module Namespace Isolation
Kita kunci satu hal penting: module tidak berbagi global scope satu sama lain.
Misalnya:
// a.zs
let A = 10
export function getA() {
    return A
}
// b.zs
let A = 20
export function getA() {
    return A
}
Keduanya punya A masing-masing.
Aturan:
Setiap module memiliki namespace/scope sendiri.
Variable yang tidak di-export tidak dapat diakses dari module lain.
Nama variable yang sama antar-module tidak menyebabkan konflik.
Hanya export dan import yang membuat binding dapat digunakan lintas module.
Tidak ada shared global variable antar-module secara default.
Built-in standard library seperti file, http, json, dan string tetap tersedia sesuai aturan runtime, bukan berasal dari global variable module pengguna.
Contoh:
// a.zs
let value = 10
// b.zs
let value = 20
Valid sepenuhnya.

--- ----

77. Module Self-Import
Kita larang module meng-import dirinya sendiri secara langsung.
Contoh:
// math.zs
import { tambah } from "./math"
→ ERROR.
Aturan:
Module meng-import dirinya sendiri → ERROR.
Self-import dianggap sebagai circular dependency.
Error harus terjadi saat dependency graph dianalisis, sebelum module dieksekusi.
Pesan error dapat menunjukkan module yang menyebabkan cycle.
Jadi:
math.zs
   ↓
math.zs
langsung ditolak.
Dengan ini bagian circular dependency jadi lebih tegas: baik cycle A → B → A maupun self-import A → A sama-sama ERROR.

------

78. Module Import Ordering
Sekarang kita kunci aturan kalau dalam satu module ada import + kode biasa.
Semua import diproses sebelum kode module lainnya dieksekusi, meskipun posisi import ditulis di tengah file.
Contoh:
print("A")

import { test } from "./utils"

print("B")
Secara execution order:
import ./utils
↓
jalankan utils.zs
↓
print("A")
↓
print("B")
Aturan:
import bersifat static module declaration.
Posisi import di source tidak menentukan kapan dependency di-load.
Seluruh dependency harus diselesaikan sebelum kode module dimulai.
Dependency yang gagal → module tidak dieksekusi.
import tidak bisa digunakan sebagai operasi runtime biasa.
import tetap harus berada di level module, bukan di dalam function, if, while, atau block lainnya.
Jadi walaupun secara visual import berada di tengah file, ZeroScript tetap memperlakukannya sebagai dependency declaration, bukan statement runtime biasa.

---

79. Module Import Duplicate

Import module yang sama berkali-kali diperbolehkan, selama tidak ada konflik binding lokal.

import { tambah } from "./math"
import { kurang } from "./math"

Valid.

Karena module hanya diinisialisasi sekali, kedua import menggunakan instance module yang sama.

Tetapi:

import { tambah } from "./math"
import { tambah } from "./math"

→ ERROR, karena tambah dideklarasikan dua kali pada scope yang sama.

Alias dapat digunakan jika memang ingin dua nama lokal:

import { tambah as add } from "./math"
import { tambah as plus } from "./math"

Valid.

Aturan utamanya:
> Module boleh di-import berulang, tetapi local binding tidak boleh dideklarasikan dua kali.

---

80. Module System Completion

Kita jadikan ini penutup aturan inti Module System.
Module ZeroScript sekarang memiliki aturan lengkap untuk:
named import
named export
export alias / import alias
re-export
module scope & encapsulation
live binding
module initialization
initialization order
duplicate import handling
module path resolution
missing module/export errors
circular dependency
self-import
import declaration scope
import ordering
module initialization failure


Default export belum digunakan dan tetap tidak termasuk dalam sistem module saat ini.

Prinsip akhirnya:
> Dependency harus eksplisit, module terisolasi, initialization deterministik, dan tidak ada fallback tersembunyi.

Dengan 80, Module System kita anggap LOCKED. 🔒

---
