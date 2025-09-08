const fs = require('fs');
const detectIndent = require('detect-indent');
const detectNewline = require('detect-newline');

const visit = require('./visit');

const DEFAULT_INDENT_SIZE = 2;

/**
 * Overwrite file with sorted json
 * @param {String} path                  - absolutePath
 * @param {Object} [options = {}]        - optional params
 * @returns {*}
 */
function overwriteFile(path, options) {
  let fileContent = null;
  let newData = null;

  try {
    fileContent = fs.readFileSync(path, 'utf8');
    newData = visit(JSON.parse(fileContent), options);
  } catch (e) {
    console.error('Failed to retrieve json object from file');
    throw e;
  }

  let indent;

  if (options && options.indentSize) {
    indent = options.indentSize;
  } else {
    indent = detectIndent(fileContent).indent || DEFAULT_INDENT_SIZE;
  }

  const newLine = detectNewline(fileContent) || '\n';
  let newFileContent = JSON.stringify(newData, null, indent);

  if (!(options && options.noFinalNewLine)) {
    // Append a new line at EOF
    newFileContent += '\n';
  }

  if (newLine !== '\n') {
    newFileContent = newFileContent.replace(/\n/g, newLine);
  }

  fs.writeFileSync(path, newFileContent, 'utf8');
  return newData;
}

/**
 * Sorts the files json with the visit function and then overwrites the file with sorted json
 * @see visit
 * @param {String|Array} absolutePaths   - String: Absolute path to json file to sort and overwrite
 *                                         Array: Absolute paths to json files to sort and overwrite
 * @param {Object} [options = {}]        - Optional parameters object, see visit for details
 * @returns {*}                          - Whatever is returned by visit
 */
function overwrite(absolutePaths, options) {
  const paths = Array.isArray(absolutePaths) ? absolutePaths : [absolutePaths];
  const results = paths.map(path => overwriteFile(path, options));
  return results.length > 1 ? results : results[0];
}

module.exports = overwrite;
