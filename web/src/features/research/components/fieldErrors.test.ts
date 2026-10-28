import { describe, expect, it } from "vitest";
import { ApiError } from "../../../lib/api";
import { fieldMessage, formErrors } from "./fieldErrors";

describe("fieldMessage", () => {
  it.each([
    ["Input should be between -50 and 100", "Harus antara −50 dan 100"],
    ["Input should be an integer between 1 and 20", "Harus antara 1 dan 20, bilangan bulat"],
    ["Input should be a number", "Harus angka"],
    ["Input should be greater than 0", "Harus di atas nol"],
    ["Input should be a date (YYYY-MM-DD)", "Tulis tanggal seperti 2026-10-24"],
    ["Input should be an http or https URL of at most 500 characters", "Tempel satu URL http(s), paling banyak 500 karakter"],
    ["A document cannot be dated in the future", "Tanggal tidak boleh di masa depan"],
    ["Text must be at most 20000 characters", "Teks paling banyak 20000 karakter"],
    ["No such claim for this ticker", "Klaim ini tidak ada untuk emiten ini"],
    ["Not a KPI of this ticker's pack", "Bukan KPI paket emiten ini"],
    ["Give exactly one of claim_id and kpi_key", "Pilih satu target: klaim atau KPI"],
    ["Only PDF and PPTX files are accepted", "Hanya berkas PDF atau PPTX"],
    ["Field required", "Wajib diisi"],
    ["Input should be 'presentasi', 'update_operasional', 'laporan', 'artikel', 'catatan' or 'lainnya'", "Pilihan tidak dikenal"],
  ])("%s → %s", (message, text) => expect(fieldMessage(message)).toBe(text));

  it("keeps a message it does not know", () => {
    expect(fieldMessage("Something new")).toBe("Something new");
  });
});

describe("formErrors", () => {
  const unprocessable = (fields: Record<string, string>) => new ApiError("validation", "Unprocessable", { status: 422, fields });

  it("maps shown fields, renames backend keys, and puts the rest on the form so nothing is lost", () => {
    const err = unprocessable({ url: "Input should be an http or https URL of at most 500 characters", document_id: "No such document" });
    expect(formErrors(err, ["paste"], { url: "paste" })).toEqual({
      paste: "Tempel satu URL http(s), paling banyak 500 karakter",
      _: "Dokumen ini tidak ada lagi",
    });
  });

  it("a failure without field errors is one form error with its message", () => {
    expect(formErrors(new Error("Server tidak menjawab"), ["paste"])).toEqual({ _: "Server tidak menjawab" });
    expect(formErrors("boom", ["paste"], {}, "Gagal menautkan")).toEqual({ _: "Gagal menautkan" });
  });
});
