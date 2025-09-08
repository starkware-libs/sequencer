"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.TypeFingerprinter = void 0;
const crypto = require("node:crypto");
const spec = require("@jsii/spec");
/**
 * Return a fingerprint for a type.
 *
 * The fingerprint will change if the API of the given type changes.
 *
 * The fingerprint is an approximation, it's not exhaustive. It will not trace
 * into types from assemblies it can't see, for example. For the purposes of Rosetta,
 * we'll assume this is Good Enough™.
 */
class TypeFingerprinter {
    constructor(assemblies) {
        this.cache = new Map();
        this.assemblies = new Map();
        for (const assembly of assemblies) {
            this.assemblies.set(assembly.name, assembly);
        }
    }
    /**
     * Return a single fingerprint that encompasses all fqns in the list
     */
    fingerprintAll(fqns) {
        const hash = crypto.createHash('sha256');
        for (const fqn of fqns) {
            hash.update(this.fingerprintType(fqn));
        }
        return hash.digest('hex');
    }
    /**
     * Return the fingerprint for the given FQN in the assembly of this fingerprinter
     *
     * The fingerprint is always going to contain the FQN, even if the type doesn't exist
     * in this assembly.
     */
    fingerprintType(fqn) {
        return this.doFingerprint(fqn, new Set([fqn]));
    }
    doFingerprint(fqn, recursionBreaker) {
        // eslint-disable-next-line @typescript-eslint/no-this-alias
        const self = this;
        const existing = this.cache.get(fqn);
        if (existing) {
            return existing;
        }
        const hash = crypto.createHash('sha256');
        hash.update(fqn);
        const type = this.findType(fqn);
        if (type) {
            hash.update(type.kind);
            switch (type.kind) {
                case spec.TypeKind.Enum:
                    for (const member of sortedByName(type.members)) {
                        hash.update(member.name);
                    }
                    break;
                case spec.TypeKind.Class:
                case spec.TypeKind.Interface:
                    if (type.kind === spec.TypeKind.Class) {
                        visitType(type.base);
                        visitCallable(type.initializer);
                    }
                    for (const prop of sortedByName(type.properties ?? [])) {
                        hash.update(prop.name);
                        visitBools(prop.immutable, prop.static, prop.optional, prop.protected);
                        visitTypeReference(prop.type);
                    }
                    for (const method of sortedByName(type.methods ?? [])) {
                        hash.update(method.name);
                        visitCallable(method);
                        visitBools(method.returns?.optional);
                        visitTypeReference(method.returns?.type);
                    }
                    for (const implint of type.interfaces ?? []) {
                        visitType(implint);
                    }
                    break;
            }
        }
        const ret = hash.digest('hex');
        this.cache.set(fqn, ret);
        return ret;
        function visitType(fqnStr) {
            if (!fqnStr) {
                return;
            }
            if (recursionBreaker.has(fqnStr)) {
                hash.update('$RECURSION$');
                return;
            }
            recursionBreaker.add(fqnStr);
            hash.update(self.doFingerprint(fqnStr, recursionBreaker));
            recursionBreaker.delete(fqnStr);
        }
        function visitCallable(callable) {
            if (!callable) {
                return;
            }
            visitBools(callable.protected);
            for (const param of callable.parameters ?? []) {
                visitBools(param.optional, param.variadic);
                visitTypeReference(param.type);
            }
        }
        function visitTypeReference(typeRef) {
            if (!typeRef) {
                return;
            }
            if (spec.isPrimitiveTypeReference(typeRef)) {
                hash.update(typeRef.primitive);
            }
            if (spec.isNamedTypeReference(typeRef)) {
                visitType(typeRef.fqn);
            }
            if (spec.isCollectionTypeReference(typeRef)) {
                hash.update(typeRef.collection.kind);
                visitTypeReference(typeRef.collection.elementtype);
            }
            if (spec.isUnionTypeReference(typeRef)) {
                for (const t of typeRef.union.types) {
                    visitTypeReference(t);
                }
            }
        }
        function visitBools(...vs) {
            hash.update(vs.map((v) => (v ? '1' : '0')).join(''));
        }
    }
    findType(fqn) {
        const assemblyName = fqn.split('.')[0];
        return this.assemblies.get(assemblyName)?.types?.[fqn];
    }
}
exports.TypeFingerprinter = TypeFingerprinter;
function sortedByName(xs) {
    xs.sort((a, b) => a.name.localeCompare(b.name));
    return xs;
}
//# sourceMappingURL=fingerprinting.js.map