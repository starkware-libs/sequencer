"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports._child_process = void 0;
/****************************************************************************************
 * Expose `child_process` via our own object that can be easily patched by jest for tests.
 * Consumers of the `child_process` module should add functions to this object and import it
 * wherever needed.
 */
const child_process_1 = require("child_process");
exports._child_process = {
    spawnSync: child_process_1.spawnSync,
};
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiX2NoaWxkX3Byb2Nlc3MuanMiLCJzb3VyY2VSb290IjoiIiwic291cmNlcyI6WyIuLi9zcmMvX2NoaWxkX3Byb2Nlc3MudHMiXSwibmFtZXMiOltdLCJtYXBwaW5ncyI6Ijs7O0FBQUE7Ozs7R0FJRztBQUNILGlEQUEwQztBQUU3QixRQUFBLGNBQWMsR0FBRztJQUM1QixTQUFTLEVBQUUseUJBQVM7Q0FDckIsQ0FBQyIsInNvdXJjZXNDb250ZW50IjpbIi8qKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqKioqXG4gKiBFeHBvc2UgYGNoaWxkX3Byb2Nlc3NgIHZpYSBvdXIgb3duIG9iamVjdCB0aGF0IGNhbiBiZSBlYXNpbHkgcGF0Y2hlZCBieSBqZXN0IGZvciB0ZXN0cy5cbiAqIENvbnN1bWVycyBvZiB0aGUgYGNoaWxkX3Byb2Nlc3NgIG1vZHVsZSBzaG91bGQgYWRkIGZ1bmN0aW9ucyB0byB0aGlzIG9iamVjdCBhbmQgaW1wb3J0IGl0XG4gKiB3aGVyZXZlciBuZWVkZWQuXG4gKi9cbmltcG9ydCB7IHNwYXduU3luYyB9IGZyb20gJ2NoaWxkX3Byb2Nlc3MnO1xuXG5leHBvcnQgY29uc3QgX2NoaWxkX3Byb2Nlc3MgPSB7XG4gIHNwYXduU3luYzogc3Bhd25TeW5jLFxufTtcbiJdfQ==