// ZeroScript Demo — satu file, semua fitur utama bahasa.
// Jalankan: zsc examples/demo.zs

// === 1. Variabel, tipe, dan ekspresi ===

let nama = "ZeroScript"
const VERSI = "0.1.0"
let angka = 42
let desimal = 3.5
let aktif = true
let kosong = null

print(nama + " v" + VERSI)
print("tipe: " + type(nama) + ", " + type(angka) + ", " + type(aktif) + ", " + type(kosong))
print("aritmetika: " + string(1 + 2 * 3) + ", modulo: " + string(7 % 3))
print("ternary: " + string(angka > 40 ? "besar" : "kecil"))
print("konversi: " + string(number("42") + 8) + " " + string(boolean(true)))

// === 2. Kontrol alur ===

let i = 0
while i < 3 {
    i += 1
}
print("while: " + string(i))

let hitung = 0
loop {
    hitung += 1
    if hitung == 3 { break }
}
print("loop + break: " + string(hitung))

let kelipatan = []
let n = 1
while n < 5 {
    list.add(kelipatan, n * 2)
    n += 1
}
print("while + list.add ->")
print(kelipatan)

if angka > 40 {
    print("if: " + nama + " besar")
} else {
    print("else: tidak tercapai")
}

let hari = 2
switch hari {
    case 1 {
        print("case 1")
    }
    case 2 {
        print("case 2: Selasa")
    }
    default {
        print("default")
    }
}

let k = 10
k++
k -= 3
k += 5
print("compound (++, -=, +=): " + string(k))

// === 3. List dan object ===

let buah = ["apel", "pisang"]
list.add(buah, "ceri")
print("list: ")
print(buah)
print("panjang: " + string(list.length(buah)) + ", pertama: " + buah[0])
list.remove(buah, 1)
print("setelah remove index 1: ")
print(buah)

let user = { nama: "Kael", umur: 18 }
print("object: ")
print(user)
user.umur = 19
user.kota = "Jakarta"
object.remove(user, "kota")
print("setelah update + object.remove: ")
print(user)
print("akses properti: " + user.nama)

// === 4. String utilities ===

let kalimat = "  Halo, ZeroScript!  "
print("trim: [" + string.trim(kalimat) + "]")
print("upper: " + string.upper("abc") + ", length: " + string(string.length("abc")))
print("replace: " + string.replace("hello world", "world", "ZS"))
print("contains: " + string(boolean(string.contains("hello", "ell"))))
print("split: ")
print(string.split("a,b,c", ","))

// === 5. Fungsi, closure, default parameter, arrow ===

function tambah(a, b) {
    return a + b
}
print("fungsi: " + string(tambah(2, 3)))

function sapa(tujuan = "dunia") {
    return "Halo, " + tujuan + "!"
}
print(sapa())
print(sapa("ZeroScript"))

function pembuat(prefix) {
    return function(n) {
        return prefix + string(n)
    }
}
let tag = pembuat("#")
print("closure: " + tag(7))

let g = (a) => a * 2
let d = (x = 5) => x + 1
print("arrow: " + string(g(21)) + ", default arrow: " + string(d()) + " / " + string(d(9)))

// === 6. Penanganan error ===
// ZeroScript tidak punya pernyataan `throw` — error runtime (dibagi nol,
// index di luar batas, ...) dibuat oleh VM dan bisa ditangkap try/catch.

function bagi(a, b) {
    return a / b
}

try {
    print(bagi(10, 0))
} catch e {
    print("catch (" + e.type + "): " + e.message)
} finally {
    print("finally selalu jalan")
}
print("setelah try: " + string(bagi(10, 2)))

print.err("pesan ini masuk kanal stderr")

// === 7. Async / await ===
// Top-level tidak blocking (§112): pemanggilan main() memulai task, lalu
// baris-baris berikut tetap dieksekusi; hasil await muncul di akhir output.

async function pekerja(n) {
    await task.sleep(1)
    return n * 2
}

async function main() {
    let satu = pekerja(21)
    let dua = pekerja(22)
    let hasil = await task.all([satu, dua])
    print("async: satu=" + string(await satu) + ", all=")
    print(hasil)
}
main()

// === 8. JSON ===

let data = { nama: "Kael", umur: 18, tags: ["a", "b"] }
let teks = json.stringify(data)
print("json.stringify: " + teks)
let kembali = json.parse(teks)
print("json.parse: " + kembali.nama + " (" + string(kembali.umur) + ")")

// === 9. Warna ANSI ===

print(color.green("hijau") + " " + color.bold("tebal") + " " + color.yellow("kuning"))

// === 10. System info ===

print("os: " + system.os() + ", arch: " + system.arch())
print("args (tanpa argumen program): ")
print(system.args())

// === 11. type() pada koleksi ===

print("type list: " + type([]) + ", type object: " + type({}))

print("=== selesai ===")
