module.exports = {
    "roots": [
        "<rootDir>"
    ],
    moduleFileExtensions: ["ts", "tsx", "js", "mjs", "cjs", "jsx", "json", "node"],
    testMatch: [ '**/*.test.ts'],
    "transform": {
        "^.+\\.tsx?$": "ts-jest"
    },
}
