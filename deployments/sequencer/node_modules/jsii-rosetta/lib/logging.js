"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.level = exports.LEVEL_VERBOSE = exports.LEVEL_INFO = exports.Level = void 0;
exports.configure = configure;
exports.warn = warn;
exports.error = error;
exports.info = info;
exports.debug = debug;
const util = require("node:util");
var Level;
(function (Level) {
    Level[Level["ERROR"] = -2] = "ERROR";
    Level[Level["WARN"] = -1] = "WARN";
    Level[Level["QUIET"] = 0] = "QUIET";
    Level[Level["INFO"] = 1] = "INFO";
    Level[Level["VERBOSE"] = 2] = "VERBOSE";
})(Level || (exports.Level = Level = {}));
exports.LEVEL_INFO = Level.INFO;
exports.LEVEL_VERBOSE = Level.VERBOSE;
/** The minimal logging level for messages to be emitted. */
exports.level = Level.QUIET;
function configure({ level: newLevel }) {
    exports.level = newLevel;
}
function warn(fmt, ...args) {
    log(Level.WARN, fmt, ...args);
}
function error(fmt, ...args) {
    log(Level.ERROR, fmt, ...args);
}
function info(fmt, ...args) {
    log(Level.INFO, fmt, ...args);
}
function debug(fmt, ...args) {
    log(Level.VERBOSE, fmt, ...args);
}
function log(messageLevel, fmt, ...args) {
    if (exports.level >= messageLevel) {
        const levelName = Level[messageLevel];
        // `console.error` will automatically be transported from worker child to worker parent,
        // process.stderr.write() won't.
        console.error(`[jsii-rosetta] [${levelName}] ${util.format(fmt, ...args)}`);
    }
}
//# sourceMappingURL=logging.js.map