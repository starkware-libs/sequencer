#!/usr/bin/env node

// Core dependencies
const path = require('path');

// NPM dependencies
const minimist = require('minimist');
const sortJson = require('./');

const alias = {
  depth: ['d'],
  reverse: ['r'],
  ignoreCase: ['ignore-case', 'i'],
  indentSize: ['indent-size', 'spaces'],
  noFinalNewLine: ['no-final-newline', 'nn'],
};

const argv = minimist(process.argv.slice(2), { alias });

// Get all the files
const files = argv._.filter(arg => arg.endsWith('.json') || arg.endsWith('.rc'));

sortJson.overwrite(files.map(file => path.resolve(file)), argv);
