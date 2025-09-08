"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.getTypeRelatedNode = exports.getPropertyRelatedNode = exports.getParameterRelatedNode = exports.getMethodRelatedNode = exports.getInterfaceRelatedNode = exports.getEnumRelatedNode = exports.getClassOrInterfaceRelatedNode = exports.getClassRelatedNode = exports.setPropertyRelatedNode = exports.setParameterRelatedNode = exports.setMethodRelatedNode = exports.setInterfaceRelatedNode = exports.setEnumRelatedNode = exports.setClassRelatedNode = void 0;
exports.setRelatedNode = setRelatedNode;
exports.getRelatedNode = getRelatedNode;
/**
 * This module provides typed method that can be used to access TypeScript Nodes
 * that are externally related to jsii assembly entities. This is backed by a
 * `WeakMap` so that attached metadata can be garbage collected once the note
 * they have been related to is no longer reachable.
 *
 * Methods have distinctive names based on the assembly node type they work with
 * because of how TypeScript does structural - and not nominal - type checking,
 * maling it impossible to use function overrides without having small
 * interfaces excessively match all types (e.g: spec.EnumMember is largely
 * equivalent to "anything that has a name").
 */
const STORAGE = new WeakMap();
//#region Attaching nodes
const setter = (object, node) => {
    return setRelatedNode(object, node);
};
function setRelatedNode(object, node) {
    if (node != null) {
        STORAGE.set(object, node);
    }
    return object;
}
exports.setClassRelatedNode = setter;
exports.setEnumRelatedNode = setter;
exports.setInterfaceRelatedNode = setter;
exports.setMethodRelatedNode = setter;
exports.setParameterRelatedNode = setter;
exports.setPropertyRelatedNode = setter;
//#endregion
//#region Inspecting attached nodes
function getRelatedNode(object) {
    return STORAGE.get(object);
}
exports.getClassRelatedNode = getRelatedNode;
exports.getClassOrInterfaceRelatedNode = getRelatedNode;
exports.getEnumRelatedNode = getRelatedNode;
exports.getInterfaceRelatedNode = getRelatedNode;
exports.getMethodRelatedNode = STORAGE.get.bind(STORAGE);
exports.getParameterRelatedNode = getRelatedNode;
exports.getPropertyRelatedNode = getRelatedNode;
exports.getTypeRelatedNode = getRelatedNode;
//#endregion
//# sourceMappingURL=node-bindings.js.map