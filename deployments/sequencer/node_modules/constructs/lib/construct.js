"use strict";
var _a, _b, _c;
Object.defineProperty(exports, "__esModule", { value: true });
exports.RootConstruct = exports.ConstructOrder = exports.Construct = exports.Node = void 0;
const JSII_RTTI_SYMBOL_1 = Symbol.for("jsii.rtti");
const dependency_1 = require("./dependency");
const stack_trace_1 = require("./private/stack-trace");
const uniqueid_1 = require("./private/uniqueid");
const CONSTRUCT_SYM = Symbol.for('constructs.Construct');
/**
 * Represents the construct node in the scope tree.
 */
class Node {
    /**
     * Returns the node associated with a construct.
     * @param construct the construct
     *
     * @deprecated use `construct.node` instead
     */
    static of(construct) {
        return construct.node;
    }
    constructor(host, scope, id) {
        this.host = host;
        this._locked = false; // if this is "true", addChild will fail
        this._children = {};
        this._context = {};
        this._metadata = new Array();
        this._dependencies = new Set();
        this._validations = new Array();
        id = id ?? ''; // if undefined, convert to empty string
        this.id = sanitizeId(id);
        this.scope = scope;
        if (scope && !this.id) {
            throw new Error('Only root constructs may have an empty ID');
        }
        // add to parent scope
        scope?.node.addChild(host, this.id);
    }
    /**
     * The full, absolute path of this construct in the tree.
     *
     * Components are separated by '/'.
     */
    get path() {
        const components = [];
        for (const scope of this.scopes) {
            if (scope.node.id) {
                components.push(scope.node.id);
            }
        }
        return components.join(Node.PATH_SEP);
    }
    /**
     * Returns an opaque tree-unique address for this construct.
     *
     * Addresses are 42 characters hexadecimal strings. They begin with "c8"
     * followed by 40 lowercase hexadecimal characters (0-9a-f).
     *
     * Addresses are calculated using a SHA-1 of the components of the construct
     * path.
     *
     * To enable refactoring of construct trees, constructs with the ID `Default`
     * will be excluded from the calculation. In those cases constructs in the
     * same tree may have the same address.
     *
     * @example c83a2846e506bcc5f10682b564084bca2d275709ee
     */
    get addr() {
        if (!this._addr) {
            this._addr = (0, uniqueid_1.addressOf)(this.scopes.map(c => c.node.id));
        }
        return this._addr;
    }
    /**
     * Return a direct child by id, or undefined
     *
     * @param id Identifier of direct child
     * @returns the child if found, or undefined
     */
    tryFindChild(id) {
        return this._children[sanitizeId(id)];
    }
    /**
     * Return a direct child by id
     *
     * Throws an error if the child is not found.
     *
     * @param id Identifier of direct child
     * @returns Child with the given id.
     */
    findChild(id) {
        const ret = this.tryFindChild(id);
        if (!ret) {
            throw new Error(`No child with id: '${id}'`);
        }
        return ret;
    }
    /**
     * Returns the child construct that has the id `Default` or `Resource"`.
     * This is usually the construct that provides the bulk of the underlying functionality.
     * Useful for modifications of the underlying construct that are not available at the higher levels.
     *
     * @throws if there is more than one child
     * @returns a construct or undefined if there is no default child
     */
    get defaultChild() {
        if (this._defaultChild !== undefined) {
            return this._defaultChild;
        }
        const resourceChild = this.tryFindChild('Resource');
        const defaultChild = this.tryFindChild('Default');
        if (resourceChild && defaultChild) {
            throw new Error(`Cannot determine default child for ${this.path}. There is both a child with id "Resource" and id "Default"`);
        }
        return defaultChild || resourceChild;
    }
    /**
     * Override the defaultChild property.
     *
     * This should only be used in the cases where the correct
     * default child is not named 'Resource' or 'Default' as it
     * should be.
     *
     * If you set this to undefined, the default behavior of finding
     * the child named 'Resource' or 'Default' will be used.
     */
    set defaultChild(value) {
        this._defaultChild = value;
    }
    /**
     * All direct children of this construct.
     */
    get children() {
        return Object.values(this._children);
    }
    /**
     * Return this construct and all of its children in the given order
     */
    findAll(order = ConstructOrder.PREORDER) {
        const ret = new Array();
        visit(this.host);
        return ret;
        function visit(c) {
            if (order === ConstructOrder.PREORDER) {
                ret.push(c);
            }
            for (const child of c.node.children) {
                visit(child);
            }
            if (order === ConstructOrder.POSTORDER) {
                ret.push(c);
            }
        }
    }
    /**
     * This can be used to set contextual values.
     * Context must be set before any children are added, since children may consult context info during construction.
     * If the key already exists, it will be overridden.
     * @param key The context key
     * @param value The context value
     */
    setContext(key, value) {
        if (this.children.length > 0) {
            const names = this.children.map(c => c.node.id);
            throw new Error('Cannot set context after children have been added: ' + names.join(','));
        }
        this._context[key] = value;
    }
    /**
     * Retrieves a value from tree context if present. Otherwise, would throw an error.
     *
     * Context is usually initialized at the root, but can be overridden at any point in the tree.
     *
     * @param key The context key
     * @returns The context value or throws error if there is no context value for this key
     */
    getContext(key) {
        const value = this._context[key];
        if (value !== undefined) {
            return value;
        }
        if (value === undefined && !this.scope?.node) {
            throw new Error(`No context value present for ${key} key`);
        }
        return this.scope && this.scope.node.getContext(key);
    }
    /**
     * Retrieves the all context of a node from tree context.
     *
     * Context is usually initialized at the root, but can be overridden at any point in the tree.
     *
     * @param defaults Any keys to override the retrieved context
     * @returns The context object or an empty object if there is discovered context
     */
    getAllContext(defaults) {
        return this.scopes.reverse()
            .reduce((a, s) => ({ ...(s.node._context), ...a }), { ...defaults });
    }
    /**
     * Retrieves a value from tree context.
     *
     * Context is usually initialized at the root, but can be overridden at any point in the tree.
     *
     * @param key The context key
     * @returns The context value or `undefined` if there is no context value for this key.
     */
    tryGetContext(key) {
        const value = this._context[key];
        if (value !== undefined) {
            return value;
        }
        return this.scope && this.scope.node.tryGetContext(key);
    }
    /**
     * An immutable array of metadata objects associated with this construct.
     * This can be used, for example, to implement support for deprecation notices, source mapping, etc.
     */
    get metadata() {
        return [...this._metadata];
    }
    /**
     * Adds a metadata entry to this construct.
     * Entries are arbitrary values and will also include a stack trace to allow tracing back to
     * the code location for when the entry was added. It can be used, for example, to include source
     * mapping in CloudFormation templates to improve diagnostics.
     * Note that construct metadata is not the same as CloudFormation resource metadata and is never written to the CloudFormation template.
     * The metadata entries are written to the Cloud Assembly Manifest if the `treeMetadata` property is specified in the props of the App that contains this Construct.
     *
     * @param type a string denoting the type of metadata
     * @param data the value of the metadata (can be a Token). If null/undefined, metadata will not be added.
     * @param options options
     */
    addMetadata(type, data, options = {}) {
        if (data == null) {
            return;
        }
        const shouldTrace = options.stackTrace ?? false;
        const trace = shouldTrace ? (0, stack_trace_1.captureStackTrace)(options.traceFromFunction ?? this.addMetadata) : undefined;
        this._metadata.push({ type, data, trace });
    }
    /**
     * All parent scopes of this construct.
     *
     * @returns a list of parent scopes. The last element in the list will always
     * be the current construct and the first element will be the root of the
     * tree.
     */
    get scopes() {
        const ret = new Array();
        let curr = this.host;
        while (curr) {
            ret.unshift(curr);
            curr = curr.node.scope;
        }
        return ret;
    }
    /**
     * Returns the root of the construct tree.
     * @returns The root of the construct tree.
     */
    get root() {
        return this.scopes[0];
    }
    /**
     * Returns true if this construct or the scopes in which it is defined are
     * locked.
     */
    get locked() {
        if (this._locked) {
            return true;
        }
        if (this.scope && this.scope.node.locked) {
            return true;
        }
        return false;
    }
    /**
     * Add an ordering dependency on another construct.
     *
     * An `IDependable`
     */
    addDependency(...deps) {
        for (const d of deps) {
            this._dependencies.add(d);
        }
    }
    /**
     * Return all dependencies registered on this node (non-recursive).
     */
    get dependencies() {
        const result = new Array();
        for (const dep of this._dependencies) {
            for (const root of dependency_1.Dependable.of(dep).dependencyRoots) {
                result.push(root);
            }
        }
        return result;
    }
    /**
     * Remove the child with the given name, if present.
     *
     * @returns Whether a child with the given name was deleted.
     */
    tryRemoveChild(childName) {
        if (!(childName in this._children)) {
            return false;
        }
        delete this._children[childName];
        return true;
    }
    /**
     * Adds a validation to this construct.
     *
     * When `node.validate()` is called, the `validate()` method will be called on
     * all validations and all errors will be returned.
     *
     * @param validation The validation object
     */
    addValidation(validation) {
        this._validations.push(validation);
    }
    /**
     * Validates this construct.
     *
     * Invokes the `validate()` method on all validations added through
     * `addValidation()`.
     *
     * @returns an array of validation error messages associated with this
     * construct.
     */
    validate() {
        return this._validations.flatMap(v => v.validate());
    }
    /**
     * Locks this construct from allowing more children to be added. After this
     * call, no more children can be added to this construct or to any children.
     */
    lock() {
        this._locked = true;
    }
    /**
     * Adds a child construct to this node.
     *
     * @param child The child construct
     * @param childName The type name of the child construct.
     * @returns The resolved path part name of the child
     */
    addChild(child, childName) {
        if (this.locked) {
            // special error if root is locked
            if (!this.path) {
                throw new Error('Cannot add children during synthesis');
            }
            throw new Error(`Cannot add children to "${this.path}" during synthesis`);
        }
        if (this._children[childName]) {
            const name = this.id ?? '';
            const typeName = this.host.constructor.name;
            throw new Error(`There is already a Construct with name '${childName}' in ${typeName}${name.length > 0 ? ' [' + name + ']' : ''}`);
        }
        this._children[childName] = child;
    }
}
exports.Node = Node;
_a = JSII_RTTI_SYMBOL_1;
Node[_a] = { fqn: "constructs.Node", version: "10.4.2" };
/**
 * Separator used to delimit construct path components.
 */
Node.PATH_SEP = '/';
/**
 * Represents the building block of the construct graph.
 *
 * All constructs besides the root construct must be created within the scope of
 * another construct.
 */
class Construct {
    /**
     * Checks if `x` is a construct.
     *
     * Use this method instead of `instanceof` to properly detect `Construct`
     * instances, even when the construct library is symlinked.
     *
     * Explanation: in JavaScript, multiple copies of the `constructs` library on
     * disk are seen as independent, completely different libraries. As a
     * consequence, the class `Construct` in each copy of the `constructs` library
     * is seen as a different class, and an instance of one class will not test as
     * `instanceof` the other class. `npm install` will not create installations
     * like this, but users may manually symlink construct libraries together or
     * use a monorepo tool: in those cases, multiple copies of the `constructs`
     * library can be accidentally installed, and `instanceof` will behave
     * unpredictably. It is safest to avoid using `instanceof`, and using
     * this type-testing method instead.
     *
     * @returns true if `x` is an object created from a class which extends `Construct`.
     * @param x Any object
     */
    static isConstruct(x) {
        return x && typeof x === 'object' && x[CONSTRUCT_SYM];
    }
    /**
     * Creates a new construct node.
     *
     * @param scope The scope in which to define this construct
     * @param id The scoped construct ID. Must be unique amongst siblings. If
     * the ID includes a path separator (`/`), then it will be replaced by double
     * dash `--`.
     */
    constructor(scope, id) {
        this.node = new Node(this, scope, id);
        // implement IDependable privately
        dependency_1.Dependable.implement(this, {
            dependencyRoots: [this],
        });
    }
    /**
     * Returns a string representation of this construct.
     */
    toString() {
        return this.node.path || '<root>';
    }
}
exports.Construct = Construct;
_b = JSII_RTTI_SYMBOL_1;
Construct[_b] = { fqn: "constructs.Construct", version: "10.4.2" };
/**
 * In what order to return constructs
 */
var ConstructOrder;
(function (ConstructOrder) {
    /**
     * Depth-first, pre-order
     */
    ConstructOrder[ConstructOrder["PREORDER"] = 0] = "PREORDER";
    /**
     * Depth-first, post-order (leaf nodes first)
     */
    ConstructOrder[ConstructOrder["POSTORDER"] = 1] = "POSTORDER";
})(ConstructOrder || (exports.ConstructOrder = ConstructOrder = {}));
const PATH_SEP_REGEX = new RegExp(`${Node.PATH_SEP}`, 'g');
/**
 * Return a sanitized version of an arbitrary string, so it can be used as an ID
 */
function sanitizeId(id) {
    // Escape path seps as double dashes
    return id.replace(PATH_SEP_REGEX, '--');
}
// Mark all instances of 'Construct'
Object.defineProperty(Construct.prototype, CONSTRUCT_SYM, {
    value: true,
    enumerable: false,
    writable: false,
});
/**
 * Creates a new root construct node.
 *
 * The root construct represents the top of the construct tree and is not contained within a parent scope itself.
 * For root constructs, the id is optional.
 */
class RootConstruct extends Construct {
    /**
     * Creates a new root construct node.
     *
     * @param id The scoped construct ID. Must be unique amongst siblings. If
     * the ID includes a path separator (`/`), then it will be replaced by double
     * dash `--`.
     */
    constructor(id) {
        super(undefined, id ?? '');
    }
}
exports.RootConstruct = RootConstruct;
_c = JSII_RTTI_SYMBOL_1;
RootConstruct[_c] = { fqn: "constructs.RootConstruct", version: "10.4.2" };
//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJmaWxlIjoiY29uc3RydWN0LmpzIiwic291cmNlUm9vdCI6IiIsInNvdXJjZXMiOlsiLi4vc3JjL2NvbnN0cnVjdC50cyJdLCJuYW1lcyI6W10sIm1hcHBpbmdzIjoiOzs7OztBQUFBLDZDQUF1RDtBQUV2RCx1REFBMEQ7QUFDMUQsaURBQStDO0FBRS9DLE1BQU0sYUFBYSxHQUFHLE1BQU0sQ0FBQyxHQUFHLENBQUMsc0JBQXNCLENBQUMsQ0FBQztBQVl6RDs7R0FFRztBQUNILE1BQWEsSUFBSTtJQU1mOzs7OztPQUtHO0lBQ0ksTUFBTSxDQUFDLEVBQUUsQ0FBQyxTQUFxQjtRQUNwQyxPQUFPLFNBQVMsQ0FBQyxJQUFJLENBQUM7SUFDeEIsQ0FBQztJQXlCRCxZQUFvQyxJQUFlLEVBQUUsS0FBaUIsRUFBRSxFQUFVO1FBQTlDLFNBQUksR0FBSixJQUFJLENBQVc7UUFUM0MsWUFBTyxHQUFHLEtBQUssQ0FBQyxDQUFDLHdDQUF3QztRQUNoRCxjQUFTLEdBQWlDLEVBQUcsQ0FBQztRQUM5QyxhQUFRLEdBQTJCLEVBQUcsQ0FBQztRQUN2QyxjQUFTLEdBQUcsSUFBSSxLQUFLLEVBQWlCLENBQUM7UUFDdkMsa0JBQWEsR0FBRyxJQUFJLEdBQUcsRUFBZSxDQUFDO1FBRXZDLGlCQUFZLEdBQUcsSUFBSSxLQUFLLEVBQWUsQ0FBQztRQUl2RCxFQUFFLEdBQUcsRUFBRSxJQUFJLEVBQUUsQ0FBQyxDQUFDLHdDQUF3QztRQUV2RCxJQUFJLENBQUMsRUFBRSxHQUFHLFVBQVUsQ0FBQyxFQUFFLENBQUMsQ0FBQztRQUN6QixJQUFJLENBQUMsS0FBSyxHQUFHLEtBQUssQ0FBQztRQUVuQixJQUFJLEtBQUssSUFBSSxDQUFDLElBQUksQ0FBQyxFQUFFLEVBQUUsQ0FBQztZQUN0QixNQUFNLElBQUksS0FBSyxDQUFDLDJDQUEyQyxDQUFDLENBQUM7UUFDL0QsQ0FBQztRQUVELHNCQUFzQjtRQUN0QixLQUFLLEVBQUUsSUFBSSxDQUFDLFFBQVEsQ0FBQyxJQUFJLEVBQUUsSUFBSSxDQUFDLEVB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LENBQUMsQ0FBQztJQUNMLENBQUM7SUFFRDs7T0FFRztJQUNJLFFBQVE7UUFDYixPQUFPLElBQUksQ0FBQyxJQUFJLENBQUMsSUFBSSxJQUFJLFFBQVEsQ0FBQztJQUNwQyxDQUFDOztBQXBESCw4QkFxREM7OztBQWlCRDs7R0FFRztBQUNILElBQVksY0FVWDtBQVZELFdBQVksY0FBYztJQUN4Qjs7T0FFRztJQUNILDJEQUFRLENBQUE7SUFFUjs7T0FFRztJQUNILDZEQUFTLENBQUE7QUFDWCxDQUFDLEVBVlcsY0FBYyw4QkFBZCxjQUFjLFFBVXpCO0FBRUQsTUFBTSxjQUFjLEdBQUcsSUFBSSxNQUFNLENBQUMsR0FBRyxJQUFJLENBQUMsUUFBUSxFQUFFLEVBQUUsR0FBRyxDQUFDLENBQUM7QUFFM0Q7O0dBRUc7QUFDSCxTQUFTLFVBQVUsQ0FBQyxFQUFVO0lBQzVCLG9DQUFvQztJQUNwQyxPQUFPLEVBQUUsQ0FBQyxPQUFPLENBQUMsY0FBYyxFQUFFLElBQUksQ0FBQyxDQUFDO0FBQzFDLENBQUM7QUFzQkQsb0NBQW9DO0FBQ3BDLE1BQU0sQ0FBQyxjQUFjLENBQUMsU0FBUyxDQUFDLFNBQVMsRUFBRSxhQUFhLEVBQUU7SUFDeEQsS0FBSyxFQUFFLElBQUk7SUFDWCxVQUFVLEVBQUUsS0FBSztJQUNqQixRQUFRLEVBQUUsS0FBSztDQUNoQixDQUFDLENBQUM7QUFFSDs7Ozs7R0FLRztBQUNILE1BQWEsYUFBYyxTQUFRLFNBQVM7SUFDMUM7Ozs7OztPQU1HO0lBQ0gsWUFBWSxFQUFXO1FBQ3JCLEtBQUssQ0FBQyxTQUFnQixFQUFFLEVBQUUsSUFBSSxFQUFFLENBQUMsQ0FBQztJQUNwQyxDQUFDOztBQVZILHNDQVdDIiwic291cmNlc0NvbnRlbnQiOlsiaW1wb3J0IHsgRGVwZW5kYWJsZSwgSURlcGVuZGFibGUgfSBmcm9tICcuL2RlcGVuZGVuY3knO1xuaW1wb3J0IHsgTWV0YWRhdGFFbnRyeSB9IGZyb20gJy4vbWV0YWRhdGEnO1xuaW1wb3J0IHsgY2FwdHVyZVN0YWNrVHJhY2UgfSBmcm9tICcuL3ByaXZhdGUvc3RhY2stdHJhY2UnO1xuaW1wb3J0IHsgYWRkcmVzc09mIH0gZnJvbSAnLi9wcml2YXRlL3VuaXF1ZWlkJztcblxuY29uc3QgQ09OU1RSVUNUX1NZTSA9IFN5bWJvbC5mb3IoJ2NvbnN0cnVjdHMuQ29uc3RydWN0Jyk7XG5cbi8qKlxuICogUmVwcmVzZW50cyBhIGNvbnN0cnVjdC5cbiAqL1xuZXhwb3J0IGludGVyZmFjZSBJQ29uc3RydWN0IGV4dGVuZHMgSURlcGVuZGFibGUge1xuICAvKipcbiAgICogVGhlIHRyZWUgbm9kZS5cbiAgICovXG4gIHJlYWRvbmx5IG5vZGU6IE5vZGU7XG59XG5cbi8qKlxuICogUmVwcmVzZW50cyB0aGUgY29uc3RydWN0IG5vZGUgaW4gdGhlIHNjb3BlIHRyZWUuXG4gKi9cbmV4cG9ydCBjbGFzcyBOb2RlIHtcbiAgLyoqXG4gICAqIFNlcGFyYXRvciB1c2VkIHRvIGRlbGltaXQgY29uc3RydWN0IHBhdGggY29tcG9uZW50cy5cbiAgICovXG4gIHB1YmxpYyBzdGF0aWMgcmVhZG9ubHkgUEFUSF9TRVAgPSAnLyc7XG5cbiAgLyoqXG4gICAqIFJldHVybnMgdGhlIG5vZGUgYXNzb2NpYXRlZCB3aXRoIGEgY29uc3RydWN0LlxuICAgKiBAcGFyYW0gY29uc3RydWN0IHRoZSBjb25zdHJ1Y3RcbiAgICpcbiAgICogQGRlcHJlY2F0ZWQgdXNlIGBjb25zdHJ1Y3Qubm9kZWAgaW5zdGVhZFxuICAgKi9cbiAgcHVibGljIHN0YXRpYyBvZihjb25zdHJ1Y3Q6IElDb25zdHJ1Y3QpOiBOb2RlIHtcbiAgICByZXR1cm4gY29uc3RydWN0Lm5vZGU7XG4gIH1cblxuICAvKipcbiAgICogUmV0dXJucyB0aGUgc2NvcGUgaW4gd2hpY2ggdGhpcyBjb25zdHJ1Y3QgaXMgZGVmaW5lZC5cbiAgICpcbiAgICogVGhlIHZhbHVlIGlzIGB1bmRlZmluZWRgIGF0IHRoZSByb290IG9mIHRoZSBjb25zdHJ1Y3Qgc2NvcGUgdHJlZS5cbiAgICovXG4gIHB1YmxpYyByZWFkb25seSBzY29wZT86IElDb25zdHJ1Y3Q7XG5cbiAgLyoqXG4gICAqIFRoZSBpZCBvZiB0aGlzIGNvbnN0cnVjdCB3aXRoaW4gdGhlIGN1cnJlbnQgc2NvcGUuXG4gICAqXG4gICAqIFRoaXMgaXMgYSBzY29wZS11bmlxdWUgaWQuIFRvIG9idGFpbiBhbiBhcHAtdW5pcXVlIGlkIGZvciB0aGlzIGNvbnN0cnVjdCwgdXNlIGBhZGRyYC5cbiAgICovXG4gIHB1YmxpYyByZWFkb25seSBpZDogc3RyaW5nO1xuXG4gIHByaXZhdGUgX2xvY2tlZCA9IGZhbHNlOyAvLyBpZiB0aGlzIGlzIFwidHJ1ZVwiLCBhZGRDaGlsZCB3aWxsIGZhaWxcbiAgcHJpdmF0ZSByZWFkb25seSBfY2hpbGRyZW46IHsgW2lkOiBzdHJpbmddOiBJQ29uc3RydWN0IH0gPSB7IH07XG4gIHByaXZhdGUgcmVhZG9ubHkgX2NvbnRleHQ6IHsgW2tleTogc3RyaW5nXTogYW55IH0gPSB7IH07XG4gIHByaXZhdGUgcmVhZG9ubHkgX21ldGFkYXRhID0gbmV3IEFycmF5PE1ldGFkYXRhRW50cnk+KCk7XG4gIHByaXZhdGUgcmVhZG9ubHkgX2RlcGVuZGVuY2llcyA9IG5ldyBTZXQ8SURlcGVuZGFibGU+KCk7XG4gIHByaXZhdGUgX2RlZmF1bHRDaGlsZDogSUNvbnN0cnVjdCB8IHVuZGVmaW5lZDtcbiAgcHJpdmF0ZSByZWFkb25seSBfdmFsaWRhdGlvbnMgPSBuZXcgQXJyYXk8SVZhbGlkYXRpb24+KCk7XG4gIHByaXZhdGUgX2FkZHI/OiBzdHJpbmc7IC8vIGNhY2hlXG5cbiAgcHVibGljIGNvbnN0cnVjdG9yKHByaXZhdGUgcmVhZG9ubHkgaG9zdDogQ29uc3RydWN0LCBzY29wZTogSUNvbnN0cnVjdCwgaWQ6IHN0cmluZykge1xuICAgIGlkID0gaWQgPz8gJyc7IC8vIGlmIHVuZGVmaW5lZCwgY29udmVydCB0byBlbXB0eSBzdHJpbmdcblxuICAgIHRoaXMuaWQgPSBzYW5pdGl6ZUlkKGlkKTtcbiAgICB0aGlzLnNjb3BlID0gc2NvcGU7XG5cbiAgICBpZiAoc2NvcGUgJiYgIXRoaXMuaWQpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcignT25seSByb290IGNvbnN0cnVjdHMgbWF5IGhhdmUgYW4gZW1wdHkgSUQnKTtcbiAgICB9XG5cbiAgICAvLyBhZGQgdG8gcGFyZW50IHNjb3BlXG4gICAgc2NvcGU/Lm5vZGUuYWRkQ2hpbGQoaG9zdCwgdGhpcy5pZCk7XG4gIH1cblxuICAvKipcbiAgICogVGhlIGZ1bGwsIGFic29sdXRlIHBhdGggb2YgdGhpcyBjb25zdHJ1Y3QgaW4gdGhlIHRyZWUuXG4gICAqXG4gICAqIENvbXBvbmVudHMgYXJlIHNlcGFyYXRlZCBieSAnLycuXG4gICAqL1xuICBwdWJsaWMgZ2V0IHBhdGgoKTogc3RyaW5nIHtcbiAgICBjb25zdCBjb21wb25lbnRzID0gW107XG4gICAgZm9yIChjb25zdCBzY29wZSBvZiB0aGlzLnNjb3Blcykge1xuICAgICAgaWYgKHNjb3BlLm5vZGUuaWQpIHtcbiAgICAgICAgY29tcG9uZW50cy5wdXNoKHNjb3BlLm5vZGUuaWQpO1xuICAgICAgfVxuICAgIH1cbiAgICByZXR1cm4gY29tcG9uZW50cy5qb2luKE5vZGUuUEFUSF9TRVApO1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybnMgYW4gb3BhcXVlIHRyZWUtdW5pcXVlIGFkZHJlc3MgZm9yIHRoaXMgY29uc3RydWN0LlxuICAgKlxuICAgKiBBZGRyZXNzZXMgYXJlIDQyIGNoYXJhY3RlcnMgaGV4YWRlY2ltYWwgc3RyaW5ncy4gVGhleSBiZWdpbiB3aXRoIFwiYzhcIlxuICAgKiBmb2xsb3dlZCBieSA0MCBsb3dlcmNhc2UgaGV4YWRlY2ltYWwgY2hhcmFjdGVycyAoMC05YS1mKS5cbiAgICpcbiAgICogQWRkcmVzc2VzIGFyZSBjYWxjdWxhdGVkIHVzaW5nIGEgU0hBLTEgb2YgdGhlIGNvbXBvbmVudHMgb2YgdGhlIGNvbnN0cnVjdFxuICAgKiBwYXRoLlxuICAgKlxuICAgKiBUbyBlbmFibGUgcmVmYWN0b3Jpbmcgb2YgY29uc3RydWN0IHRyZWVzLCBjb25zdHJ1Y3RzIHdpdGggdGhlIElEIGBEZWZhdWx0YFxuICAgKiB3aWxsIGJlIGV4Y2x1ZGVkIGZyb20gdGhlIGNhbGN1bGF0aW9uLiBJbiB0aG9zZSBjYXNlcyBjb25zdHJ1Y3RzIGluIHRoZVxuICAgKiBzYW1lIHRyZWUgbWF5IGhhdmUgdGhlIHNhbWUgYWRkcmVzcy5cbiAgICpcbiAgICogQGV4YW1wbGUgYzgzYTI4NDZlNTA2YmNjNWYxMDY4MmI1NjQwODRiY2EyZDI3NTcwOWVlXG4gICAqL1xuICBwdWJsaWMgZ2V0IGFkZHIoKTogc3RyaW5nIHtcbiAgICBpZiAoIXRoaXMuX2FkZHIpIHtcbiAgICAgIHRoaXMuX2FkZHIgPSBhZGRyZXNzT2YodGhpcy5zY29wZXMubWFwKGMgPT4gYy5ub2RlLmlkKSk7XG4gICAgfVxuXG4gICAgcmV0dXJuIHRoaXMuX2FkZHI7XG4gIH1cblxuICAvKipcbiAgICogUmV0dXJuIGEgZGlyZWN0IGNoaWxkIGJ5IGlkLCBvciB1bmRlZmluZWRcbiAgICpcbiAgICogQHBhcmFtIGlkIElkZW50aWZpZXIgb2YgZGlyZWN0IGNoaWxkXG4gICAqIEByZXR1cm5zIHRoZSBjaGlsZCBpZiBmb3VuZCwgb3IgdW5kZWZpbmVkXG4gICAqL1xuICBwdWJsaWMgdHJ5RmluZENoaWxkKGlkOiBzdHJpbmcpOiBJQ29uc3RydWN0IHwgdW5kZWZpbmVkIHtcbiAgICByZXR1cm4gdGhpcy5fY2hpbGRyZW5bc2FuaXRpemVJZChpZCldO1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybiBhIGRpcmVjdCBjaGlsZCBieSBpZFxuICAgKlxuICAgKiBUaHJvd3MgYW4gZXJyb3IgaWYgdGhlIGNoaWxkIGlzIG5vdCBmb3VuZC5cbiAgICpcbiAgICogQHBhcmFtIGlkIElkZW50aWZpZXIgb2YgZGlyZWN0IGNoaWxkXG4gICAqIEByZXR1cm5zIENoaWxkIHdpdGggdGhlIGdpdmVuIGlkLlxuICAgKi9cbiAgcHVibGljIGZpbmRDaGlsZChpZDogc3RyaW5nKTogSUNvbnN0cnVjdCB7XG4gICAgY29uc3QgcmV0ID0gdGhpcy50cnlGaW5kQ2hpbGQoaWQpO1xuICAgIGlmICghcmV0KSB7XG4gICAgICB0aHJvdyBuZXcgRXJyb3IoYE5vIGNoaWxkIHdpdGggaWQ6ICcke2lkfSdgKTtcbiAgICB9XG4gICAgcmV0dXJuIHJldDtcbiAgfVxuXG4gIC8qKlxuICAgKiBSZXR1cm5zIHRoZSBjaGlsZCBjb25zdHJ1Y3QgdGhhdCBoYXMgdGhlIGlkIGBEZWZhdWx0YCBvciBgUmVzb3VyY2VcImAuXG4gICAqIFRoaXMgaXMgdXN1YWxseSB0aGUgY29uc3RydWN0IHRoYXQgcHJvdmlkZXMgdGhlIGJ1bGsgb2YgdGhlIHVuZGVybHlpbmcgZnVuY3Rpb25hbGl0eS5cbiAgICogVXNlZnVsIGZvciBtb2RpZmljYXRpb25zIG9mIHRoZSB1bmRlcmx5aW5nIGNvbnN0cnVjdCB0aGF0IGFyZSBub3QgYXZhaWxhYmxlIGF0IHRoZSBoaWdoZXIgbGV2ZWxzLlxuICAgKlxuICAgKiBAdGhyb3dzIGlmIHRoZXJlIGlzIG1vcmUgdGhhbiBvbmUgY2hpbGRcbiAgICogQHJldHVybnMgYSBjb25zdHJ1Y3Qgb3IgdW5kZWZpbmVkIGlmIHRoZXJlIGlzIG5vIGRlZmF1bHQgY2hpbGRcbiAgICovXG4gIHB1YmxpYyBnZXQgZGVmYXVsdENoaWxkKCk6IElDb25zdHJ1Y3QgfCB1bmRlZmluZWQge1xuICAgIGlmICh0aGlzLl9kZWZhdWx0Q2hpbGQgIT09IHVuZGVmaW5lZCkge1xuICAgICAgcmV0dXJuIHRoaXMuX2RlZmF1bHRDaGlsZDtcbiAgICB9XG5cbiAgICBjb25zdCByZXNvdXJjZUNoaWxkID0gdGhpcy50cnlGaW5kQ2hpbGQoJ1Jlc291cmNlJyk7XG4gICAgY29uc3QgZGVmYXVsdENoaWxkID0gdGhpcy50cnlGaW5kQ2hpbGQoJ0RlZmF1bHQnKTtcbiAgICBpZiAocmVzb3VyY2VDaGlsZCAmJiBkZWZhdWx0Q2hpbGQpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcihgQ2Fubm90IGRldGVybWluZSBkZWZhdWx0IGNoaWxkIGZvciAke3RoaXMucGF0aH0uIFRoZXJlIGlzIGJvdGggYSBjaGlsZCB3aXRoIGlkIFwiUmVzb3VyY2VcIiBhbmQgaWQgXCJEZWZhdWx0XCJgKTtcbiAgICB9XG5cbiAgICByZXR1cm4gZGVmYXVsdENoaWxkIHx8IHJlc291cmNlQ2hpbGQ7XG4gIH1cblxuICAvKipcbiAgICogT3ZlcnJpZGUgdGhlIGRlZmF1bHRDaGlsZCBwcm9wZXJ0eS5cbiAgICpcbiAgICogVGhpcyBzaG91bGQgb25seSBiZSB1c2VkIGluIHRoZSBjYXNlcyB3aGVyZSB0aGUgY29ycmVjdFxuICAgKiBkZWZhdWx0IGNoaWxkIGlzIG5vdCBuYW1lZCAnUmVzb3VyY2UnIG9yICdEZWZhdWx0JyBhcyBpdFxuICAgKiBzaG91bGQgYmUuXG4gICAqXG4gICAqIElmIHlvdSBzZXQgdGhpcyB0byB1bmRlZmluZWQsIHRoZSBkZWZhdWx0IGJlaGF2aW9yIG9mIGZpbmRpbmdcbiAgICogdGhlIGNoaWxkIG5hbWVkICdSZXNvdXJjZScgb3IgJ0RlZmF1bHQnIHdpbGwgYmUgdXNlZC5cbiAgICovXG4gIHB1YmxpYyBzZXQgZGVmYXVsdENoaWxkKHZhbHVlOiBJQ29uc3RydWN0IHwgdW5kZWZpbmVkKSB7XG4gICAgdGhpcy5fZGVmYXVsdENoaWxkID0gdmFsdWU7XG4gIH1cblxuICAvKipcbiAgICogQWxsIGRpcmVjdCBjaGlsZHJlbiBvZiB0aGlzIGNvbnN0cnVjdC5cbiAgICovXG4gIHB1YmxpYyBnZXQgY2hpbGRyZW4oKSB7XG4gICAgcmV0dXJuIE9iamVjdC52YWx1ZXModGhpcy5fY2hpbGRyZW4pO1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybiB0aGlzIGNvbnN0cnVjdCBhbmQgYWxsIG9mIGl0cyBjaGlsZHJlbiBpbiB0aGUgZ2l2ZW4gb3JkZXJcbiAgICovXG4gIHB1YmxpYyBmaW5kQWxsKG9yZGVyOiBDb25zdHJ1Y3RPcmRlciA9IENvbnN0cnVjdE9yZGVyLlBSRU9SREVSKTogSUNvbnN0cnVjdFtdIHtcbiAgICBjb25zdCByZXQgPSBuZXcgQXJyYXk8SUNvbnN0cnVjdD4oKTtcbiAgICB2aXNpdCh0aGlzLmhvc3QpO1xuICAgIHJldHVybiByZXQ7XG5cbiAgICBmdW5jdGlvbiB2aXNpdChjOiBJQ29uc3RydWN0KSB7XG4gICAgICBpZiAob3JkZXIgPT09IENvbnN0cnVjdE9yZGVyLlBSRU9SREVSKSB7XG4gICAgICAgIHJldC5wdXNoKGMpO1xuICAgICAgfVxuXG4gICAgICBmb3IgKGNvbnN0IGNoaWxkIG9mIGMubm9kZS5jaGlsZHJlbikge1xuICAgICAgICB2aXNpdChjaGlsZCk7XG4gICAgICB9XG5cbiAgICAgIGlmIChvcmRlciA9PT0gQ29uc3RydWN0T3JkZXIuUE9TVE9SREVSKSB7XG4gICAgICAgIHJldC5wdXNoKGMpO1xuICAgICAgfVxuICAgIH1cbiAgfVxuXG4gIC8qKlxuICAgKiBUaGlzIGNhbiBiZSB1c2VkIHRvIHNldCBjb250ZXh0dWFsIHZhbHVlcy5cbiAgICogQ29udGV4dCBtdXN0IGJlIHNldCBiZWZvcmUgYW55IGNoaWxkcmVuIGFyZSBhZGRlZCwgc2luY2UgY2hpbGRyZW4gbWF5IGNvbnN1bHQgY29udGV4dCBpbmZvIGR1cmluZyBjb25zdHJ1Y3Rpb24uXG4gICAqIElmIHRoZSBrZXkgYWxyZWFkeSBleGlzdHMsIGl0IHdpbGwgYmUgb3ZlcnJpZGRlbi5cbiAgICogQHBhcmFtIGtleSBUaGUgY29udGV4dCBrZXlcbiAgICogQHBhcmFtIHZhbHVlIFRoZSBjb250ZXh0IHZhbHVlXG4gICAqL1xuICBwdWJsaWMgc2V0Q29udGV4dChrZXk6IHN0cmluZywgdmFsdWU6IGFueSkge1xuICAgIGlmICh0aGlzLmNoaWxkcmVuLmxlbmd0aCA+IDApIHtcbiAgICAgIGNvbnN0IG5hbWVzID0gdGhpcy5jaGlsZHJlbi5tYXAoYyA9PiBjLm5vZGUuaWQpO1xuICAgICAgdGhyb3cgbmV3IEVycm9yKCdDYW5ub3Qgc2V0IGNvbnRleHQgYWZ0ZXIgY2hpbGRyZW4gaGF2ZSBiZWVuIGFkZGVkOiAnICsgbmFtZXMuam9pbignLCcpKTtcbiAgICB9XG4gICAgdGhpcy5fY29udGV4dFtrZXldID0gdmFsdWU7XG4gIH1cblxuICAvKipcbiAgICogUmV0cmlldmVzIGEgdmFsdWUgZnJvbSB0cmVlIGNvbnRleHQgaWYgcHJlc2VudC4gT3RoZXJ3aXNlLCB3b3VsZCB0aHJvdyBhbiBlcnJvci5cbiAgICpcbiAgICogQ29udGV4dCBpcyB1c3VhbGx5IGluaXRpYWxpemVkIGF0IHRoZSByb290LCBidXQgY2FuIGJlIG92ZXJyaWRkZW4gYXQgYW55IHBvaW50IGluIHRoZSB0cmVlLlxuICAgKlxuICAgKiBAcGFyYW0ga2V5IFRoZSBjb250ZXh0IGtleVxuICAgKiBAcmV0dXJucyBUaGUgY29udGV4dCB2YWx1ZSBvciB0aHJvd3MgZXJyb3IgaWYgdGhlcmUgaXMgbm8gY29udGV4dCB2YWx1ZSBmb3IgdGhpcyBrZXlcbiAgICovXG4gIHB1YmxpYyBnZXRDb250ZXh0KGtleTogc3RyaW5nKTogYW55IHtcbiAgICBjb25zdCB2YWx1ZSA9IHRoaXMuX2NvbnRleHRba2V5XTtcblxuICAgIGlmICh2YWx1ZSAhPT0gdW5kZWZpbmVkKSB7IHJldHVybiB2YWx1ZTsgfVxuXG4gICAgaWYgKHZhbHVlID09PSB1bmRlZmluZWQgJiYgIXRoaXMuc2NvcGU/Lm5vZGUpIHtcbiAgICAgIHRocm93IG5ldyBFcnJvcihgTm8gY29udGV4dCB2YWx1ZSBwcmVzZW50IGZvciAke2tleX0ga2V5YCk7XG4gICAgfVxuXG4gICAgcmV0dXJuIHRoaXMuc2NvcGUgJiYgdGhpcy5zY29wZS5ub2RlLmdldENvbnRleHQoa2V5KTtcbiAgfVxuXG4gIC8qKlxuICAgKiBSZXRyaWV2ZXMgdGhlIGFsbCBjb250ZXh0IG9mIGEgbm9kZSBmcm9tIHRyZWUgY29udGV4dC5cbiAgICpcbiAgICogQ29udGV4dCBpcyB1c3VhbGx5IGluaXRpYWxpemVkIGF0IHRoZSByb290LCBidXQgY2FuIGJlIG92ZXJyaWRkZW4gYXQgYW55IHBvaW50IGluIHRoZSB0cmVlLlxuICAgKlxuICAgKiBAcGFyYW0gZGVmYXVsdHMgQW55IGtleXMgdG8gb3ZlcnJpZGUgdGhlIHJldHJpZXZlZCBjb250ZXh0XG4gICAqIEByZXR1cm5zIFRoZSBjb250ZXh0IG9iamVjdCBvciBhbiBlbXB0eSBvYmplY3QgaWYgdGhlcmUgaXMgZGlzY292ZXJlZCBjb250ZXh0XG4gICAqL1xuICBwdWJsaWMgZ2V0QWxsQ29udGV4dChkZWZhdWx0cz86IG9iamVjdCk6IGFueSB7XG4gICAgcmV0dXJuIHRoaXMuc2NvcGVzLnJldmVyc2UoKVxuICAgICAgLnJlZHVjZSgoYSwgcykgPT4gKHsgLi4uKHMubm9kZS5fY29udGV4dCksIC4uLmEgfSksIHsgLi4uZGVmYXVsdHMgfSk7XG4gIH1cblxuICAvKipcbiAgICogUmV0cmlldmVzIGEgdmFsdWUgZnJvbSB0cmVlIGNvbnRleHQuXG4gICAqXG4gICAqIENvbnRleHQgaXMgdXN1YWxseSBpbml0aWFsaXplZCBhdCB0aGUgcm9vdCwgYnV0IGNhbiBiZSBvdmVycmlkZGVuIGF0IGFueSBwb2ludCBpbiB0aGUgdHJlZS5cbiAgICpcbiAgICogQHBhcmFtIGtleSBUaGUgY29udGV4dCBrZXlcbiAgICogQHJldHVybnMgVGhlIGNvbnRleHQgdmFsdWUgb3IgYHVuZGVmaW5lZGAgaWYgdGhlcmUgaXMgbm8gY29udGV4dCB2YWx1ZSBmb3IgdGhpcyBrZXkuXG4gICAqL1xuICBwdWJsaWMgdHJ5R2V0Q29udGV4dChrZXk6IHN0cmluZyk6IGFueSB7XG4gICAgY29uc3QgdmFsdWUgPSB0aGlzLl9jb250ZXh0W2tleV07XG4gICAgaWYgKHZhbHVlICE9PSB1bmRlZmluZWQpIHsgcmV0dXJuIHZhbHVlOyB9XG5cbiAgICByZXR1cm4gdGhpcy5zY29wZSAmJiB0aGlzLnNjb3BlLm5vZGUudHJ5R2V0Q29udGV4dChrZXkpO1xuICB9XG5cbiAgLyoqXG4gICAqIEFuIGltbXV0YWJsZSBhcnJheSBvZiBtZXRhZGF0YSBvYmplY3RzIGFzc29jaWF0ZWQgd2l0aCB0aGlzIGNvbnN0cnVjdC5cbiAgICogVGhpcyBjYW4gYmUgdXNlZCwgZm9yIGV4YW1wbGUsIHRvIGltcGxlbWVudCBzdXBwb3J0IGZvciBkZXByZWNhdGlvbiBub3RpY2VzLCBzb3VyY2UgbWFwcGluZywgZXRjLlxuICAgKi9cbiAgcHVibGljIGdldCBtZXRhZGF0YSgpIHtcbiAgICByZXR1cm4gWy4uLnRoaXMuX21ldGFkYXRhXTtcbiAgfVxuXG4gIC8qKlxuICAgKiBBZGRzIGEgbWV0YWRhdGEgZW50cnkgdG8gdGhpcyBjb25zdHJ1Y3QuXG4gICAqIEVudHJpZXMgYXJlIGFyYml0cmFyeSB2YWx1ZXMgYW5kIHdpbGwgYWxzbyBpbmNsdWRlIGEgc3RhY2sgdHJhY2UgdG8gYWxsb3cgdHJhY2luZyBiYWNrIHRvXG4gICAqIHRoZSBjb2RlIGxvY2F0aW9uIGZvciB3aGVuIHRoZSBlbnRyeSB3YXMgYWRkZWQuIEl0IGNhbiBiZSB1c2VkLCBmb3IgZXhhbXBsZSwgdG8gaW5jbHVkZSBzb3VyY2VcbiAgICogbWFwcGluZyBpbiBDbG91ZEZvcm1hdGlvbiB0ZW1wbGF0ZXMgdG8gaW1wcm92ZSBkaWFnbm9zdGljcy5cbiAgICogTm90ZSB0aGF0IGNvbnN0cnVjdCBtZXRhZGF0YSBpcyBub3QgdGhlIHNhbWUgYXMgQ2xvdWRGb3JtYXRpb24gcmVzb3VyY2UgbWV0YWRhdGEgYW5kIGlzIG5ldmVyIHdyaXR0ZW4gdG8gdGhlIENsb3VkRm9ybWF0aW9uIHRlbXBsYXRlLlxuICAgKiBUaGUgbWV0YWRhdGEgZW50cmllcyBhcmUgd3JpdHRlbiB0byB0aGUgQ2xvdWQgQXNzZW1ibHkgTWFuaWZlc3QgaWYgdGhlIGB0cmVlTWV0YWRhdGFgIHByb3BlcnR5IGlzIHNwZWNpZmllZCBpbiB0aGUgcHJvcHMgb2YgdGhlIEFwcCB0aGF0IGNvbnRhaW5zIHRoaXMgQ29uc3RydWN0LlxuICAgKlxuICAgKiBAcGFyYW0gdHlwZSBhIHN0cmluZyBkZW5vdGluZyB0aGUgdHlwZSBvZiBtZXRhZGF0YVxuICAgKiBAcGFyYW0gZGF0YSB0aGUgdmFsdWUgb2YgdGhlIG1ldGFkYXRhIChjYW4gYmUgYSBUb2tlbikuIElmIG51bGwvdW5kZWZpbmVkLCBtZXRhZGF0YSB3aWxsIG5vdCBiZSBhZGRlZC5cbiAgICogQHBhcmFtIG9wdGlvbnMgb3B0aW9uc1xuICAgKi9cbiAgcHVibGljIGFkZE1ldGFkYXRhKHR5cGU6IHN0cmluZywgZGF0YTogYW55LCBvcHRpb25zOiBNZXRhZGF0YU9wdGlvbnMgPSB7IH0pOiB2b2lkIHtcbiAgICBpZiAoZGF0YSA9PSBudWxsKSB7XG4gICAgICByZXR1cm47XG4gICAgfVxuXG4gICAgY29uc3Qgc2hvdWxkVHJhY2UgPSBvcHRpb25zLnN0YWNrVHJhY2UgPz8gZmFsc2U7XG4gICAgY29uc3QgdHJhY2UgPSBzaG91bGRUcmFjZSA/IGNhcHR1cmVTdGFja1RyYWNlKG9wdGlvbnMudHJhY2VGcm9tRnVuY3Rpb24gPz8gdGhpcy5hZGRNZXRhZGF0YSkgOiB1bmRlZmluZWQ7XG4gICAgdGhpcy5fbWV0YWRhdGEucHVzaCh7IHR5cGUsIGRhdGEsIHRyYWNlIH0pO1xuICB9XG5cbiAgLyoqXG4gICAqIEFsbCBwYXJlbnQgc2NvcGVzIG9mIHRoaXMgY29uc3RydWN0LlxuICAgKlxuICAgKiBAcmV0dXJucyBhIGxpc3Qgb2YgcGFyZW50IHNjb3Blcy4gVGhlIGxhc3QgZWxlbWVudCBpbiB0aGUgbGlzdCB3aWxsIGFsd2F5c1xuICAgKiBiZSB0aGUgY3VycmVudCBjb25zdHJ1Y3QgYW5kIHRoZSBmaXJzdCBlbGVtZW50IHdpbGwgYmUgdGhlIHJvb3Qgb2YgdGhlXG4gICAqIHRyZWUuXG4gICAqL1xuICBwdWJsaWMgZ2V0IHNjb3BlcygpOiBJQ29uc3RydWN0W10ge1xuICAgIGNvbnN0IHJldCA9IG5ldyBBcnJheTxJQ29uc3RydWN0PigpO1xuXG4gICAgbGV0IGN1cnI6IElDb25zdHJ1Y3QgfCB1bmRlZmluZWQgPSB0aGlzLmhvc3Q7XG4gICAgd2hpbGUgKGN1cnIpIHtcbiAgICAgIHJldC51bnNoaWZ0KGN1cnIpO1xuICAgICAgY3VyciA9IGN1cnIubm9kZS5zY29wZTtcbiAgICB9XG5cbiAgICByZXR1cm4gcmV0O1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybnMgdGhlIHJvb3Qgb2YgdGhlIGNvbnN0cnVjdCB0cmVlLlxuICAgKiBAcmV0dXJucyBUaGUgcm9vdCBvZiB0aGUgY29uc3RydWN0IHRyZWUuXG4gICAqL1xuICBwdWJsaWMgZ2V0IHJvb3QoKSB7XG4gICAgcmV0dXJuIHRoaXMuc2NvcGVzWzBdO1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybnMgdHJ1ZSBpZiB0aGlzIGNvbnN0cnVjdCBvciB0aGUgc2NvcGVzIGluIHdoaWNoIGl0IGlzIGRlZmluZWQgYXJlXG4gICAqIGxvY2tlZC5cbiAgICovXG4gIHB1YmxpYyBnZXQgbG9ja2VkKCkge1xuICAgIGlmICh0aGlzLl9sb2NrZWQpIHtcbiAgICAgIHJldHVybiB0cnVlO1xuICAgIH1cblxuICAgIGlmICh0aGlzLnNjb3BlICYmIHRoaXMuc2NvcGUubm9kZS5sb2NrZWQpIHtcbiAgICAgIHJldHVybiB0cnVlO1xuICAgIH1cblxuICAgIHJldHVybiBmYWxzZTtcbiAgfVxuXG4gIC8qKlxuICAgKiBBZGQgYW4gb3JkZXJpbmcgZGVwZW5kZW5jeSBvbiBhbm90aGVyIGNvbnN0cnVjdC5cbiAgICpcbiAgICogQW4gYElEZXBlbmRhYmxlYFxuICAgKi9cbiAgcHVibGljIGFkZERlcGVuZGVuY3koLi4uZGVwczogSURlcGVuZGFibGVbXSkge1xuICAgIGZvciAoY29uc3QgZCBvZiBkZXBzKSB7XG4gICAgICB0aGlzLl9kZXBlbmRlbmNpZXMuYWRkKGQpO1xuICAgIH1cbiAgfVxuXG4gIC8qKlxuICAgKiBSZXR1cm4gYWxsIGRlcGVuZGVuY2llcyByZWdpc3RlcmVkIG9uIHRoaXMgbm9kZSAobm9uLXJlY3Vyc2l2ZSkuXG4gICAqL1xuICBwdWJsaWMgZ2V0IGRlcGVuZGVuY2llcygpOiBJQ29uc3RydWN0W10ge1xuICAgIGNvbnN0IHJlc3VsdCA9IG5ldyBBcnJheTxJQ29uc3RydWN0PigpO1xuICAgIGZvciAoY29uc3QgZGVwIG9mIHRoaXMuX2RlcGVuZGVuY2llcykge1xuICAgICAgZm9yIChjb25zdCByb290IG9mIERlcGVuZGFibGUub2YoZGVwKS5kZXBlbmRlbmN5Um9vdHMpIHtcbiAgICAgICAgcmVzdWx0LnB1c2gocm9vdCk7XG4gICAgICB9XG4gICAgfVxuXG4gICAgcmV0dXJuIHJlc3VsdDtcbiAgfVxuXG4gIC8qKlxuICAgKiBSZW1vdmUgdGhlIGNoaWxkIHdpdGggdGhlIGdpdmVuIG5hbWUsIGlmIHByZXNlbnQuXG4gICAqXG4gICAqIEByZXR1cm5zIFdoZXRoZXIgYSBjaGlsZCB3aXRoIHRoZSBnaXZlbiBuYW1lIHdhcyBkZWxldGVkLlxuICAgKi9cbiAgcHVibGljIHRyeVJlbW92ZUNoaWxkKGNoaWxkTmFtZTogc3RyaW5nKTogYm9vbGVhbiB7XG4gICAgaWYgKCEoY2hpbGROYW1lIGluIHRoaXMuX2NoaWxkcmVuKSkgeyByZXR1cm4gZmFsc2U7IH1cbiAgICBkZWxldGUgdGhpcy5fY2hpbGRyZW5bY2hpbGROYW1lXTtcbiAgICByZXR1cm4gdHJ1ZTtcbiAgfVxuXG4gIC8qKlxuICAgKiBBZGRzIGEgdmFsaWRhdGlvbiB0byB0aGlzIGNvbnN0cnVjdC5cbiAgICpcbiAgICogV2hlbiBgbm9kZS52YWxpZGF0ZSgpYCBpcyBjYWxsZWQsIHRoZSBgdmFsaWRhdGUoKWAgbWV0aG9kIHdpbGwgYmUgY2FsbGVkIG9uXG4gICAqIGFsbCB2YWxpZGF0aW9ucyBhbmQgYWxsIGVycm9ycyB3aWxsIGJlIHJldHVybmVkLlxuICAgKlxuICAgKiBAcGFyYW0gdmFsaWRhdGlvbiBUaGUgdmFsaWRhdGlvbiBvYmplY3RcbiAgICovXG4gIHB1YmxpYyBhZGRWYWxpZGF0aW9uKHZhbGlkYXRpb246IElWYWxpZGF0aW9uKSB7XG4gICAgdGhpcy5fdmFsaWRhdGlvbnMucHVzaCh2YWxpZGF0aW9uKTtcbiAgfVxuXG4gIC8qKlxuICAgKiBWYWxpZGF0ZXMgdGhpcyBjb25zdHJ1Y3QuXG4gICAqXG4gICAqIEludm9rZXMgdGhlIGB2YWxpZGF0ZSgpYCBtZXRob2Qgb24gYWxsIHZhbGlkYXRpb25zIGFkZGVkIHRocm91Z2hcbiAgICogYGFkZFZhbGlkYXRpb24oKWAuXG4gICAqXG4gICAqIEByZXR1cm5zIGFuIGFycmF5IG9mIHZhbGlkYXRpb24gZXJyb3IgbWVzc2FnZXMgYXNzb2NpYXRlZCB3aXRoIHRoaXNcbiAgICogY29uc3RydWN0LlxuICAgKi9cbiAgcHVibGljIHZhbGlkYXRlKCk6IHN0cmluZ1tdIHtcbiAgICByZXR1cm4gdGhpcy5fdmFsaWRhdGlvbnMuZmxhdE1hcCh2ID0+IHYudmFsaWRhdGUoKSk7XG4gIH1cblxuICAvKipcbiAgICogTG9ja3MgdGhpcyBjb25zdHJ1Y3QgZnJvbSBhbGxvd2luZyBtb3JlIGNoaWxkcmVuIHRvIGJlIGFkZGVkLiBBZnRlciB0aGlzXG4gICAqIGNhbGwsIG5vIG1vcmUgY2hpbGRyZW4gY2FuIGJlIGFkZGVkIHRvIHRoaXMgY29uc3RydWN0IG9yIHRvIGFueSBjaGlsZHJlbi5cbiAgICovXG4gIHB1YmxpYyBsb2NrKCkge1xuICAgIHRoaXMuX2xvY2tlZCA9IHRydWU7XG4gIH1cblxuICAvKipcbiAgICogQWRkcyBhIGNoaWxkIGNvbnN0cnVjdCB0byB0aGlzIG5vZGUuXG4gICAqXG4gICAqIEBwYXJhbSBjaGlsZCBUaGUgY2hpbGQgY29uc3RydWN0XG4gICAqIEBwYXJhbSBjaGlsZE5hbWUgVGhlIHR5cGUgbmFtZSBvZiB0aGUgY2hpbGQgY29uc3RydWN0LlxuICAgKiBAcmV0dXJucyBUaGUgcmVzb2x2ZWQgcGF0aCBwYXJ0IG5hbWUgb2YgdGhlIGNoaWxkXG4gICAqL1xuICBwcml2YXRlIGFkZENoaWxkKGNoaWxkOiBDb25zdHJ1Y3QsIGNoaWxkTmFtZTogc3RyaW5nKSB7XG4gICAgaWYgKHRoaXMubG9ja2VkKSB7XG5cbiAgICAgIC8vIHNwZWNpYWwgZXJyb3IgaWYgcm9vdCBpcyBsb2NrZWRcbiAgICAgIGlmICghdGhpcy5wYXRoKSB7XG4gICAgICAgIHRocm93IG5ldyBFcnJvcignQ2Fubm90IGFkZCBjaGlsZHJlbiBkdXJpbmcgc3ludGhlc2lzJyk7XG4gICAgICB9XG5cbiAgICAgIHRocm93IG5ldyBFcnJvcihgQ2Fubm90IGFkZCBjaGlsZHJlbiB0byBcIiR7dGhpcy5wYXRofVwiIGR1cmluZyBzeW50aGVzaXNgKTtcbiAgICB9XG5cbiAgICBpZiAodGhpcy5fY2hpbGRyZW5bY2hpbGROYW1lXSkge1xuICAgICAgY29uc3QgbmFtZSA9IHRoaXMuaWQgPz8gJyc7XG4gICAgICBjb25zdCB0eXBlTmFtZSA9IHRoaXMuaG9zdC5jb25zdHJ1Y3Rvci5uYW1lO1xuICAgICAgdGhyb3cgbmV3IEVycm9yKGBUaGVyZSBpcyBhbHJlYWR5IGEgQ29uc3RydWN0IHdpdGggbmFtZSAnJHtjaGlsZE5hbWV9JyBpbiAke3R5cGVOYW1lfSR7bmFtZS5sZW5ndGggPiAwID8gJyBbJyArIG5hbWUgKyAnXScgOiAnJ31gKTtcbiAgICB9XG5cbiAgICB0aGlzLl9jaGlsZHJlbltjaGlsZE5hbWVdID0gY2hpbGQ7XG4gIH1cbn1cblxuLyoqXG4gKiBSZXByZXNlbnRzIHRoZSBidWlsZGluZyBibG9jayBvZiB0aGUgY29uc3RydWN0IGdyYXBoLlxuICpcbiAqIEFsbCBjb25zdHJ1Y3RzIGJlc2lkZXMgdGhlIHJvb3QgY29uc3RydWN0IG11c3QgYmUgY3JlYXRlZCB3aXRoaW4gdGhlIHNjb3BlIG9mXG4gKiBhbm90aGVyIGNvbnN0cnVjdC5cbiAqL1xuZXhwb3J0IGNsYXNzIENvbnN0cnVjdCBpbXBsZW1lbnRzIElDb25zdHJ1Y3Qge1xuICAvKipcbiAgICogQ2hlY2tzIGlmIGB4YCBpcyBhIGNvbnN0cnVjdC5cbiAgICpcbiAgICogVXNlIHRoaXMgbWV0aG9kIGluc3RlYWQgb2YgYGluc3RhbmNlb2ZgIHRvIHByb3Blcmx5IGRldGVjdCBgQ29uc3RydWN0YFxuICAgKiBpbnN0YW5jZXMsIGV2ZW4gd2hlbiB0aGUgY29uc3RydWN0IGxpYnJhcnkgaXMgc3ltbGlua2VkLlxuICAgKlxuICAgKiBFeHBsYW5hdGlvbjogaW4gSmF2YVNjcmlwdCwgbXVsdGlwbGUgY29waWVzIG9mIHRoZSBgY29uc3RydWN0c2AgbGlicmFyeSBvblxuICAgKiBkaXNrIGFyZSBzZWVuIGFzIGluZGVwZW5kZW50LCBjb21wbGV0ZWx5IGRpZmZlcmVudCBsaWJyYXJpZXMuIEFzIGFcbiAgICogY29uc2VxdWVuY2UsIHRoZSBjbGFzcyBgQ29uc3RydWN0YCBpbiBlYWNoIGNvcHkgb2YgdGhlIGBjb25zdHJ1Y3RzYCBsaWJyYXJ5XG4gICAqIGlzIHNlZW4gYXMgYSBkaWZmZXJlbnQgY2xhc3MsIGFuZCBhbiBpbnN0YW5jZSBvZiBvbmUgY2xhc3Mgd2lsbCBub3QgdGVzdCBhc1xuICAgKiBgaW5zdGFuY2VvZmAgdGhlIG90aGVyIGNsYXNzLiBgbnBtIGluc3RhbGxgIHdpbGwgbm90IGNyZWF0ZSBpbnN0YWxsYXRpb25zXG4gICAqIGxpa2UgdGhpcywgYnV0IHVzZXJzIG1heSBtYW51YWxseSBzeW1saW5rIGNvbnN0cnVjdCBsaWJyYXJpZXMgdG9nZXRoZXIgb3JcbiAgICogdXNlIGEgbW9ub3JlcG8gdG9vbDogaW4gdGhvc2UgY2FzZXMsIG11bHRpcGxlIGNvcGllcyBvZiB0aGUgYGNvbnN0cnVjdHNgXG4gICAqIGxpYnJhcnkgY2FuIGJlIGFjY2lkZW50YWxseSBpbnN0YWxsZWQsIGFuZCBgaW5zdGFuY2VvZmAgd2lsbCBiZWhhdmVcbiAgICogdW5wcmVkaWN0YWJseS4gSXQgaXMgc2FmZXN0IHRvIGF2b2lkIHVzaW5nIGBpbnN0YW5jZW9mYCwgYW5kIHVzaW5nXG4gICAqIHRoaXMgdHlwZS10ZXN0aW5nIG1ldGhvZCBpbnN0ZWFkLlxuICAgKlxuICAgKiBAcmV0dXJucyB0cnVlIGlmIGB4YCBpcyBhbiBvYmplY3QgY3JlYXRlZCBmcm9tIGEgY2xhc3Mgd2hpY2ggZXh0ZW5kcyBgQ29uc3RydWN0YC5cbiAgICogQHBhcmFtIHggQW55IG9iamVjdFxuICAgKi9cbiAgcHVibGljIHN0YXRpYyBpc0NvbnN0cnVjdCh4OiBhbnkpOiB4IGlzIENvbnN0cnVjdCB7XG4gICAgcmV0dXJuIHggJiYgdHlwZW9mIHggPT09ICdvYmplY3QnICYmIHhbQ09OU1RSVUNUX1NZTV07XG4gIH1cblxuICAvKipcbiAgICogVGhlIHRyZWUgbm9kZS5cbiAgICovXG4gIHB1YmxpYyByZWFkb25seSBub2RlOiBOb2RlO1xuXG4gIC8qKlxuICAgKiBDcmVhdGVzIGEgbmV3IGNvbnN0cnVjdCBub2RlLlxuICAgKlxuICAgKiBAcGFyYW0gc2NvcGUgVGhlIHNjb3BlIGluIHdoaWNoIHRvIGRlZmluZSB0aGlzIGNvbnN0cnVjdFxuICAgKiBAcGFyYW0gaWQgVGhlIHNjb3BlZCBjb25zdHJ1Y3QgSUQuIE11c3QgYmUgdW5pcXVlIGFtb25nc3Qgc2libGluZ3MuIElmXG4gICAqIHRoZSBJRCBpbmNsdWRlcyBhIHBhdGggc2VwYXJhdG9yIChgL2ApLCB0aGVuIGl0IHdpbGwgYmUgcmVwbGFjZWQgYnkgZG91YmxlXG4gICAqIGRhc2ggYC0tYC5cbiAgICovXG4gIGNvbnN0cnVjdG9yKHNjb3BlOiBDb25zdHJ1Y3QsIGlkOiBzdHJpbmcpIHtcbiAgICB0aGlzLm5vZGUgPSBuZXcgTm9kZSh0aGlzLCBzY29wZSwgaWQpO1xuXG4gICAgLy8gaW1wbGVtZW50IElEZXBlbmRhYmxlIHByaXZhdGVseVxuICAgIERlcGVuZGFibGUuaW1wbGVtZW50KHRoaXMsIHtcbiAgICAgIGRlcGVuZGVuY3lSb290czogW3RoaXNdLFxuICAgIH0pO1xuICB9XG5cbiAgLyoqXG4gICAqIFJldHVybnMgYSBzdHJpbmcgcmVwcmVzZW50YXRpb24gb2YgdGhpcyBjb25zdHJ1Y3QuXG4gICAqL1xuICBwdWJsaWMgdG9TdHJpbmcoKSB7XG4gICAgcmV0dXJuIHRoaXMubm9kZS5wYXRoIHx8ICc8cm9vdD4nO1xuICB9XG59XG5cbi8qKlxuICogSW1wbGVtZW50IHRoaXMgaW50ZXJmYWNlIGluIG9yZGVyIGZvciB0aGUgY29uc3RydWN0IHRvIGJlIGFibGUgdG8gdmFsaWRhdGUgaXRzZWxmLlxuICovXG5leHBvcnQgaW50ZXJmYWNlIElWYWxpZGF0aW9uIHtcbiAgLyoqXG4gICAqIFZhbGlkYXRlIHRoZSBjdXJyZW50IGNvbnN0cnVjdC5cbiAgICpcbiAgICogVGhpcyBtZXRob2QgY2FuIGJlIGltcGxlbWVudGVkIGJ5IGRlcml2ZWQgY29uc3RydWN0cyBpbiBvcmRlciB0byBwZXJmb3JtXG4gICAqIHZhbGlkYXRpb24gbG9naWMuIEl0IGlzIGNhbGxlZCBvbiBhbGwgY29uc3RydWN0cyBiZWZvcmUgc3ludGhlc2lzLlxuICAgKlxuICAgKiBAcmV0dXJucyBBbiBhcnJheSBvZiB2YWxpZGF0aW9uIGVycm9yIG1lc3NhZ2VzLCBvciBhbiBlbXB0eSBhcnJheSBpZiB0aGVyZSB0aGUgY29uc3RydWN0IGlzIHZhbGlkLlxuICAgKi9cbiAgdmFsaWRhdGUoKTogc3RyaW5nW107XG59XG5cbi8qKlxuICogSW4gd2hhdCBvcmRlciB0byByZXR1cm4gY29uc3RydWN0c1xuICovXG5leHBvcnQgZW51bSBDb25zdHJ1Y3RPcmRlciB7XG4gIC8qKlxuICAgKiBEZXB0aC1maXJzdCwgcHJlLW9yZGVyXG4gICAqL1xuICBQUkVPUkRFUixcblxuICAvKipcbiAgICogRGVwdGgtZmlyc3QsIHBvc3Qtb3JkZXIgKGxlYWYgbm9kZXMgZmlyc3QpXG4gICAqL1xuICBQT1NUT1JERVJcbn1cblxuY29uc3QgUEFUSF9TRVBfUkVHRVggPSBuZXcgUmVnRXhwKGAke05vZGUuUEFUSF9TRVB9YCwgJ2cnKTtcblxuLyoqXG4gKiBSZXR1cm4gYSBzYW5pdGl6ZWQgdmVyc2lvbiBvZiBhbiBhcmJpdHJhcnkgc3RyaW5nLCBzbyBpdCBjYW4gYmUgdXNlZCBhcyBhbiBJRFxuICovXG5mdW5jdGlvbiBzYW5pdGl6ZUlkKGlkOiBzdHJpbmcpIHtcbiAgLy8gRXNjYXBlIHBhdGggc2VwcyBhcyBkb3VibGUgZGFzaGVzXG4gIHJldHVybiBpZC5yZXBsYWNlKFBBVEhfU0VQX1JFR0VYLCAnLS0nKTtcbn1cblxuLyoqXG4gKiBPcHRpb25zIGZvciBgY29uc3RydWN0LmFkZE1ldGFkYXRhKClgLlxuICovXG5leHBvcnQgaW50ZXJmYWNlIE1ldGFkYXRhT3B0aW9ucyB7XG4gIC8qKlxuICAgKiBJbmNsdWRlIHN0YWNrIHRyYWNlIHdpdGggbWV0YWRhdGEgZW50cnkuXG4gICAqIEBkZWZhdWx0IGZhbHNlXG4gICAqL1xuICByZWFkb25seSBzdGFja1RyYWNlPzogYm9vbGVhbjtcblxuICAvKipcbiAgICogQSBKYXZhU2NyaXB0IGZ1bmN0aW9uIHRvIGJlZ2luIHRyYWNpbmcgZnJvbS5cbiAgICpcbiAgICogVGhpcyBvcHRpb24gaXMgaWdub3JlZCB1bmxlc3MgYHN0YWNrVHJhY2VgIGlzIGB0cnVlYC5cbiAgICpcbiAgICogQGRlZmF1bHQgYWRkTWV0YWRhdGEoKVxuICAgKi9cbiAgcmVhZG9ubHkgdHJhY2VGcm9tRnVuY3Rpb24/OiBhbnk7XG59XG5cbi8vIE1hcmsgYWxsIGluc3RhbmNlcyBvZiAnQ29uc3RydWN0J1xuT2JqZWN0LmRlZmluZVByb3BlcnR5KENvbnN0cnVjdC5wcm90b3R5cGUsIENPTlNUUlVDVF9TWU0sIHtcbiAgdmFsdWU6IHRydWUsXG4gIGVudW1lcmFibGU6IGZhbHNlLFxuICB3cml0YWJsZTogZmFsc2UsXG59KTtcblxuLyoqXG4gKiBDcmVhdGVzIGEgbmV3IHJvb3QgY29uc3RydWN0IG5vZGUuXG4gKlxuICogVGhlIHJvb3QgY29uc3RydWN0IHJlcHJlc2VudHMgdGhlIHRvcCBvZiB0aGUgY29uc3RydWN0IHRyZWUgYW5kIGlzIG5vdCBjb250YWluZWQgd2l0aGluIGEgcGFyZW50IHNjb3BlIGl0c2VsZi5cbiAqIEZvciByb290IGNvbnN0cnVjdHMsIHRoZSBpZCBpcyBvcHRpb25hbC5cbiAqL1xuZXhwb3J0IGNsYXNzIFJvb3RDb25zdHJ1Y3QgZXh0ZW5kcyBDb25zdHJ1Y3Qge1xuICAvKipcbiAgICogQ3JlYXRlcyBhIG5ldyByb290IGNvbnN0cnVjdCBub2RlLlxuICAgKlxuICAgKiBAcGFyYW0gaWQgVGhlIHNjb3BlZCBjb25zdHJ1Y3QgSUQuIE11c3QgYmUgdW5pcXVlIGFtb25nc3Qgc2libGluZ3MuIElmXG4gICAqIHRoZSBJRCBpbmNsdWRlcyBhIHBhdGggc2VwYXJhdG9yIChgL2ApLCB0aGVuIGl0IHdpbGwgYmUgcmVwbGFjZWQgYnkgZG91YmxlXG4gICAqIGRhc2ggYC0tYC5cbiAgICovXG4gIGNvbnN0cnVjdG9yKGlkPzogc3RyaW5nKSB7XG4gICAgc3VwZXIodW5kZWZpbmVkIGFzIGFueSwgaWQgPz8gJycpO1xuICB9XG59XG4iXX0=