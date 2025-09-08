"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.addressOf = void 0;
const crypto = require("crypto");
/**
 * Resources with this ID are complete hidden from the logical ID calculation.
 */
const HIDDEN_ID = 'Default';
/**
 * Calculates the construct uid based on path components.
 *
 * Components named `Default` (case sensitive) are excluded from uid calculation
 * to allow tree refactorings.
 *
 * @param components path components
 */
function addressOf(components) {
    const hash = crypto.createHash('sha1');
    for (const c of components) {
        // skip components called "Default" to enable refactorings
        if (c === HIDDEN_ID) {
            continue;
        }
        hash.update(c);
        hash.update('\n');
    }
    // prefix with "c8" so to ensure it starts with non-digit.
    return 'c8' + hash.digest('hex');
}
exports.addressOf = addressOf;
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoidW5pcXVlaWQuanMiLCJzb3VyY2VSb290IjoiIiwic291cmNlcyI6WyIuLi8uLi9zcmMvcHJpdmF0ZS91bmlxdWVpZC50cyJdLCJuYW1lcyI6W10sIm1hcHBpbmdzIjoiOzs7QUFBQSxpQ0FBaUM7QUFFakM7O0dBRUc7QUFDSCxNQUFNLFNBQVMsR0FBRyxTQUFTLENBQUM7QUFFNUI7Ozs7Ozs7R0FPRztBQUNILFNBQWdCLFNBQVMsQ0FBQyxVQUFvQjtJQUM1QyxNQUFNLElBQUksR0FBRyxNQUFNLENBQUMsVUFBVSxDQUFDLE1BQU0sQ0FBQyxDQUFDO0lBQ3ZDLEtBQUssTUFBTSxDQUFDLElBQUksVUFBVSxFQUFFLENBQUM7UUFDM0IsMERBQTBEO1FBQzFELElBQUksQ0FBQyxLQUFLLFNBQVMsRUFBRSxDQUFDO1lBQUMsU0FBUztRQUFDLENBQUM7UUFFbEMsSUFBSSxDQUFDLE1BQU0sQ0FBQyxDQUFDLENBQUMsQ0FBQztRQUNmLElBQUksQ0FBQyxNQUFNLENBQUMsSUFBSSxDQUFDLENBQUM7SUFDcEIsQ0FBQztJQUVELDBEQUEwRDtJQUMxRCxPQUFPLElBQUksR0FBRyxJQUFJLENBQUMsTUFBTSxDQUFDLEtBQUssQ0FBQyxDQUFDO0FBQ25DLENBQUM7QUFaRCw4QkFZQyIsInNvdXJjZXNDb250ZW50IjpbImltcG9ydCAqIGFzIGNyeXB0byBmcm9tICdjcnlwdG8nO1xuXG4vKipcbiAqIFJlc291cmNlcyB3aXRoIHRoaXMgSUQgYXJlIGNvbXBsZXRlIGhpZGRlbiBmcm9tIHRoZSBsb2dpY2FsIElEIGNhbGN1bGF0aW9uLlxuICovXG5jb25zdCBISURERU5fSUQgPSAnRGVmYXVsdCc7XG5cbi8qKlxuICogQ2FsY3VsYXRlcyB0aGUgY29uc3RydWN0IHVpZCBiYXNlZCBvbiBwYXRoIGNvbXBvbmVudHMuXG4gKlxuICogQ29tcG9uZW50cyBuYW1lZCBgRGVmYXVsdGAgKGNhc2Ugc2Vuc2l0aXZlKSBhcmUgZXhjbHVkZWQgZnJvbSB1aWQgY2FsY3VsYXRpb25cbiAqIHRvIGFsbG93IHRyZWUgcmVmYWN0b3JpbmdzLlxuICpcbiAqIEBwYXJhbSBjb21wb25lbnRzIHBhdGggY29tcG9uZW50c1xuICovXG5leHBvcnQgZnVuY3Rpb24gYWRkcmVzc09mKGNvbXBvbmVudHM6IHN0cmluZ1tdKSB7XG4gIGNvbnN0IGhhc2ggPSBjcnlwdG8uY3JlYXRlSGFzaCgnc2hhMScpO1xuICBmb3IgKGNvbnN0IGMgb2YgY29tcG9uZW50cykge1xuICAgIC8vIHNraXAgY29tcG9uZW50cyBjYWxsZWQgXCJEZWZhdWx0XCIgdG8gZW5hYmxlIHJlZmFjdG9yaW5nc1xuICAgIGlmIChjID09PSBISURERU5fSUQpIHsgY29udGludWU7IH1cblxuICAgIGhhc2gudXBkYXRlKGMpO1xuICAgIGhhc2gudXBkYXRlKCdcXG4nKTtcbiAgfVxuXG4gIC8vIHByZWZpeCB3aXRoIFwiYzhcIiBzbyB0byBlbnN1cmUgaXQgc3RhcnRzIHdpdGggbm9uLWRpZ2l0LlxuICByZXR1cm4gJ2M4JyArIGhhc2guZGlnZXN0KCdoZXgnKTtcbn1cbiJdfQ==