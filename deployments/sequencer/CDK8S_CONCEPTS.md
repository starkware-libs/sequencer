# CDK8s Concepts Explained

## Hierarchy

```
App
 ├── Chart 1 (provides namespace, generates YAML directory)
 │    └── Construct (organizes resources)
 │         └── Construct (can nest deeper)
 │
 └── Chart 2 (separate namespace, separate YAML directory)
      └── Construct
```

## App

- **One per application** - the root container
- Handles synthesis (`app.synth()`) - generates YAML files
- No namespace - just the root of the tree
- **Example:** `app = App(...)`

## Chart

- **Purpose:** Groups resources under a **namespace** and organizes YAML output
- **Rule:** Must be a direct child of `App` or another `Chart`
- **Creates:** One YAML output directory per Chart
- **Provides:** Namespace that all children inherit
- **Inherits from:** `Chart` class (which inherits from `Construct`)
- **Example:** `NodeChart` (line 20 in app.py)

```python
class NodeChart(Chart):  # ✅ Correct - inherits from Chart
    def __init__(self, scope: Construct, ...):  # scope must be App or Chart
        super().__init__(scope, name, namespace=namespace)
```

## Construct

- **Purpose:** Reusable component that organizes related resources
- **Rule:** Can be a child of `Chart` or another `Construct`
- **Does NOT:** Provide namespace (inherits from parent Chart)
- **Does NOT:** Create separate YAML directory
- **Inherits from:** `Construct` class
- **Example:** `NodeConstruct`, `DeploymentConstruct`, `ServiceConstruct`

```python
class NodeConstruct(Construct):  # ✅ Correct - inherits from Construct
    def __init__(self, scope: Construct, ...):  # scope can be Chart or Construct
        super().__init__(scope, id)
        # Now create children:
        self.service = ServiceConstruct(self, "service", ...)  # 'self' is the scope
```

## Scope Explained

**Scope** = the parent container in the construct tree.

### How scope works:

1. **Every construct takes `scope` as first parameter:**
   ```python
   def __init__(self, scope: Construct, id: str, ...):
       super().__init__(scope, id)  # Registers self as child of scope
   ```

2. **When creating a child, pass `self` as scope:**
   ```python
   # In NodeConstruct:
   self.service = ServiceConstruct(
       self,        # ← 'self' (NodeConstruct) is the scope/parent
       "service",   # ← id/name of this construct
       ...
   )
   ```

3. **What children inherit from scope:**
   - Namespace (from the parent Chart)
   - Name prefix (constructs get unique names like `NodeChart-NodeConstruct-service`)
   - Organization (resources grouped together)

### Scope Hierarchy Example:

```python
# main() function:
app = App(...)                    # Root - has no scope

# Inside main():
NodeChart(scope=app, ...)          # Chart - scope is App
    │
    └── NodeConstruct(scope=self, ...)  # Construct - scope is NodeChart
            │
            ├── ServiceConstruct(scope=self, ...)    # scope is NodeConstruct
            ├── DeploymentConstruct(scope=self, ...) # scope is NodeConstruct
            └── ConfigMapConstruct(scope=self, ...) # scope is NodeConstruct
```

## Why Your Code Structure Works

### Your Current Structure:
```
App
 └── NodeChart (Chart)
      └── NodeConstruct (Construct)
           ├── ServiceConstruct
           ├── DeploymentConstruct
           └── ConfigMapConstruct
```

### Why NodeConstruct is a Construct (not Chart):

1. **Namespace:** All resources in NodeConstruct inherit namespace from NodeChart
2. **Organization:** Resources stay together in the same YAML directory
3. **Flexibility:** NodeConstruct can be reused in different Charts
4. **Tree Rules:** Only one level of Chart needed (NodeChart), then Constructs can nest

### If NodeConstruct were a Chart (WRONG):

```python
# This would be WRONG:
class NodeConstruct(Chart):  # ❌ Don't do this!
    ...
```

Problems:
- Charts can't be nested directly (only App → Chart → Chart is allowed, but creates separate namespace)
- Each Chart creates separate YAML output directory
- Resources wouldn't share namespace properly

## Quick Reference

| Type | Inherits From | Scope Can Be | Provides Namespace? | Creates YAML Dir? |
|------|---------------|--------------|---------------------|-------------------|
| **App** | - | - | ❌ | ❌ (root) |
| **Chart** | `Chart` (→ `Construct`) | `App` or `Chart` | ✅ | ✅ |
| **Construct** | `Construct` | `Chart` or `Construct` | ❌ (inherits) | ❌ |

## Naming Convention (Your Codebase)

- `*Chart` classes → inherit from `Chart` → provide namespace
- `*Construct` classes → inherit from `Construct` → organize resources

