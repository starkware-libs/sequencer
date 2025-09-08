"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.StrictBrand = void 0;
exports.startsWithUppercase = startsWithUppercase;
exports.printDiagnostics = printDiagnostics;
exports.formatList = formatList;
exports.annotateStrictDiagnostic = annotateStrictDiagnostic;
exports.hasStrictBranding = hasStrictBranding;
exports.divideEvenly = divideEvenly;
exports.flat = flat;
exports.partition = partition;
exports.setExtend = setExtend;
exports.mkDict = mkDict;
exports.fmap = fmap;
exports.mapValues = mapValues;
exports.sortBy = sortBy;
exports.groupBy = groupBy;
exports.isDefined = isDefined;
exports.indexBy = indexBy;
exports.commentToken = commentToken;
exports.pathExists = pathExists;
const node_fs_1 = require("node:fs");
function startsWithUppercase(x) {
    return /^[A-Z]/.exec(x) != null;
}
function printDiagnostics(diags, stream, colors) {
    // Don't print too much, at some point it just clogs up the log
    const maxDiags = 50;
    for (const diag of diags.slice(0, maxDiags)) {
        stream.write(colors ? diag.formattedMessage : stripColorCodes(diag.formattedMessage));
    }
    if (diags.length > maxDiags) {
        stream.write(`(...and ${diags.length - maxDiags} more diagnostics not shown)`);
    }
}
function formatList(xs, n = 5) {
    const tooMany = xs.length - n;
    return tooMany > 0 ? `${xs.slice(0, n).join(', ')} (and ${tooMany} more)` : xs.join(', ');
}
exports.StrictBrand = 'jsii.strict';
/**
 * Annotate a diagnostic with a magic property to indicate it's a strict diagnostic
 */
function annotateStrictDiagnostic(diag) {
    Object.defineProperty(diag, exports.StrictBrand, {
        configurable: false,
        enumerable: true,
        value: true,
        writable: false,
    });
}
/**
 * Return whether or not the given diagnostic was annotated with the magic strict property
 */
function hasStrictBranding(diag) {
    return !!diag[exports.StrictBrand];
}
/**
 * Chunk an array of elements into approximately equal groups
 */
function divideEvenly(groups, xs) {
    const chunkSize = Math.ceil(xs.length / groups);
    const ret = [];
    for (let i = 0; i < groups; i++) {
        ret.push(xs.slice(i * chunkSize, (i + 1) * chunkSize));
    }
    return ret;
}
function flat(xs) {
    return Array.prototype.concat.apply([], xs);
}
/**
 * Partition a list in twain using a predicate
 *
 * Returns [elements-matching-predicate, elements-not-matching-predicate];
 */
function partition(xs, pred) {
    const truthy = new Array();
    const falsy = new Array();
    for (const x of xs) {
        if (pred(x)) {
            truthy.push(x);
        }
        else {
            falsy.push(x);
        }
    }
    return [truthy, falsy];
}
function setExtend(xs, els) {
    for (const el of els) {
        xs.add(el);
    }
}
function mkDict(xs) {
    const ret = {};
    for (const [key, value] of xs) {
        ret[key] = value;
    }
    return ret;
}
function fmap(value, fn) {
    if (value == null) {
        return undefined;
    }
    return fn(value);
}
function mapValues(xs, fn) {
    const ret = {};
    for (const [key, value] of Object.entries(xs)) {
        ret[key] = fn(value);
    }
    return ret;
}
/**
 * Sort an array by a key function.
 *
 * Instead of having to write your own comparators for your types any time you
 * want to sort, you supply a function that maps a value to a compound sort key
 * consisting of numbers or strings. The sorting will happen by that sort key
 * instead.
 */
function sortBy(xs, keyFn) {
    return xs.sort((a, b) => {
        const aKey = keyFn(a);
        const bKey = keyFn(b);
        for (let i = 0; i < Math.min(aKey.length, bKey.length); i++) {
            // Compare aKey[i] to bKey[i]
            const av = aKey[i];
            const bv = bKey[i];
            if (av === bv) {
                continue;
            }
            if (typeof av !== typeof bv) {
                throw new Error(`Type of sort key ${JSON.stringify(aKey)} not same as ${JSON.stringify(bKey)}`);
            }
            if (typeof av === 'number' && typeof bv === 'number') {
                return av - bv;
            }
            if (typeof av === 'string' && typeof bv === 'string') {
                return av.localeCompare(bv);
            }
        }
        return aKey.length - bKey.length;
    });
}
/**
 * Group elements by a key
 *
 * Supply a function that maps each element to a key string.
 *
 * Returns a map of the key to the list of elements that map to that key.
 */
function groupBy(xs, keyFn) {
    const ret = {};
    for (const x of xs) {
        const key = keyFn(x);
        if (ret[key]) {
            ret[key].push(x);
        }
        else {
            ret[key] = [x];
        }
    }
    return ret;
}
function isDefined(x) {
    return x !== undefined;
}
function indexBy(xs, fn) {
    return mkDict(xs.map((x) => [fn(x), x]));
}
function commentToken(language) {
    // This is future-proofed a bit, but don't read too much in this...
    switch (language) {
        case 'python':
        case 'ruby':
            return '#';
        case 'csharp':
        case 'java':
        case 'go':
        default:
            return '//';
    }
}
async function pathExists(path) {
    try {
        await node_fs_1.promises.stat(path);
        return true;
    }
    catch (err) {
        if (err.code === 'ENOENT') {
            return false;
        }
        if (!err.stack) {
            Error.captureStackTrace(err);
        }
        throw err;
    }
}
// Copy/pasted from the 'ansi-regex' package to avoid taking a dependency for this one line that will never change
const ANSI_PATTERN = new RegExp([
    '[\\u001B\\u009B][[\\]()#;?]*(?:(?:(?:(?:;[-a-zA-Z\\d\\/#&.:=?%@~_]+)*|[a-zA-Z\\d]+(?:;[-a-zA-Z\\d\\/#&.:=?%@~_]*)*)?\\u0007)',
    '(?:(?:\\d{1,4}(?:;\\d{0,4})*)?[\\dA-PR-TZcf-nq-uy=><~]))',
].join('|'), 'g');
function stripColorCodes(x) {
    return x.replace(ANSI_PATTERN, '');
}
//# sourceMappingURL=util.js.map