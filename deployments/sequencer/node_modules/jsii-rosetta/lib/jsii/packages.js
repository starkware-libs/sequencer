"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.jsiiTargetParameter = jsiiTargetParameter;
function jsiiTargetParameter(target, field) {
    const path = field.split('.');
    let r = target.targets;
    while (path.length > 0 && typeof r === 'object' && r !== null) {
        r = r[path.splice(0, 1)[0]];
    }
    return r;
}
//# sourceMappingURL=packages.js.map