"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.level = exports.LEVEL_SILLY = exports.LEVEL_VERBOSE = exports.LEVEL_INFO = exports.Level = void 0;
exports.configure = configure;
exports.warn = warn;
exports.info = info;
exports.debug = debug;
var Level;
(function (Level) {
    Level[Level["WARN"] = -1] = "WARN";
    Level[Level["QUIET"] = 0] = "QUIET";
    Level[Level["INFO"] = 1] = "INFO";
    Level[Level["VERBOSE"] = 2] = "VERBOSE";
    Level[Level["SILLY"] = 3] = "SILLY";
})(Level || (exports.Level = Level = {}));
exports.LEVEL_INFO = Level.INFO;
exports.LEVEL_VERBOSE = Level.VERBOSE;
exports.LEVEL_SILLY = Level.SILLY;
/** The minimal logging level for messages to be emitted. */
exports.level = Level.QUIET;
function configure({ level: newLevel }) {
    exports.level = newLevel;
}
function warn(fmt, ...args) {
    log(Level.WARN, fmt, ...args);
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
        console.error(`[jsii-pacmak] [${levelName}]`, fmt, ...args);
    }
}
//# sourceMappingURL=logging.js.map