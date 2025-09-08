/* eslint-disable */
'use strict';

const fs = require('fs');
const path = require('path');
const cp = require('child_process');
const chai = require('chai');
const dirtyChai = require('dirty-chai');

const expect = chai.expect;
chai.use(dirtyChai);

const tempFile = path.resolve(__dirname, './temp_file_test.json');
const tempFile2 = path.resolve(__dirname, './temp_file_2_test.json');

afterEach(() => {
  try { fs.unlinkSync(tempFile); fs.unlinkSync(tempFile2) } catch (e) {}
});

describe('cmd', () => {
  it('sorts object by keys', () => {
    after(() => {
      try { fs.unlinkSync(tempFile); } catch (e) {}
    });

    const givenData = { foo: 123, bar: 456, baz: 789 };
    const expectedData = { bar: 456, baz: 789, foo: 123 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile}`);

    // parse then stringify to remove white space issues
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile, 'utf8')))).to.equal(JSON.stringify(expectedData));
  });

  it('sorts object in reverse by keys if reverse enabled', () => {
    const givenData = { abc: 123, def: 456, hij: 789 };
    const expectedData = { hij: 789, def: 456, abc: 123 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    fs.writeFileSync(tempFile2, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile} --reverse`);
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile2} -r`);

    // parse then stringify to remove white space issues
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile, 'utf8')))).to.equal(JSON.stringify(expectedData));
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile2, 'utf8')))).to.equal(JSON.stringify(expectedData));
  });

  it('sorts object by keys with depth not set', () => {
    const givenData = { def: 456, abc: { b: 1, a: 2 }, hij: 789 };
    const expectedData = { abc: { a: 2, b: 1 }, def: 456, hij: 789 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile}`);

    // parse then stringify to remove white space issues
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile, 'utf8')))).to.equal(JSON.stringify(expectedData));
  });

  it('sorts object by keys with depth = 1', () => {
    const givenData = { def: 456, abc: { b: 1, a: 2 }, hij: 789 };
    const expectedData = { abc: { b: 1, a: 2 }, def: 456, hij: 789 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    fs.writeFileSync(tempFile2, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile} -d=1`);
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile2} --depth=1`);

    // parse then stringify to remove white space issues
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile, 'utf8')))).to.equal(JSON.stringify(expectedData));
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile2, 'utf8')))).to.equal(JSON.stringify(expectedData));
  });

  it('sorts object by keys with depth = 2', () => {
    const givenData = { def: 456, abc: { b: 1, a: 2 }, hij: 789 };
    const expectedData = { abc: { a: 2, b: 1 }, def: 456, hij: 789 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    fs.writeFileSync(tempFile2, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile} -d=2`);
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile2} --depth=2`);

    // parse then stringify to remove white space issues
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile, 'utf8')))).to.equal(JSON.stringify(expectedData));
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile2, 'utf8')))).to.equal(JSON.stringify(expectedData));
  });

  it('sorts object by keys and ignores case if ignoreCase enabled', () => {
    const givenData = { foo: 123, bar: 456, baz: 789, Quax: 999, Foo2: 123 };
    const expectedData = { bar: 456, baz: 789, foo: 123, Foo2: 123, Quax: 999 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    fs.writeFileSync(tempFile2, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile} --ignore-case`);
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile2} -i`);

    // parse then stringify to remove white space issues
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile, 'utf8')))).to.equal(JSON.stringify(expectedData));
    expect(JSON.stringify(JSON.parse(fs.readFileSync(tempFile2, 'utf8')))).to.equal(JSON.stringify(expectedData));
  });

  it('sorts object by keys and uses given indent size', () => {
    const givenData = { foo: 123, bar: 456 };

    fs.writeFileSync(tempFile, JSON.stringify(givenData), 'utf8');
    cp.execSync(`node ${path.resolve(__dirname, '../app/cmd')} ${tempFile} --indent-size=3`);

    // parse then stringify to remove white space issues
    const expectedData = { bar: 456, foo: 123};
    const expectedFileContent = JSON.stringify(expectedData, null, 3) + '\n';
    expect(fs.readFileSync(tempFile, 'utf8')).to.equal(expectedFileContent);
  });
});
