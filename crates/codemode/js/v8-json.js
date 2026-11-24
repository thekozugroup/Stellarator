// V8's JSON.parse error messages.
//
// Models call JSON.parse on text that is not JSON, and a model that sends invalid JSON as a tool
// call's arguments gets the SyntaxError's text back through the AI SDK. Both texts are V8's in
// Cloudflare's Code Mode, and QuickJS words them differently. V8's JSON parser has a small, closed
// set of messages (src/json/json-parser.cc and src/common/message-template.h, V8 12 to 14), so
// they are reproduced here exactly: `__v8JsonParseError(text)` returns the message V8's
// `JSON.parse(text)` throws, or null when `text` is valid JSON. The scanner below follows V8's
// parser step by step (which token it looks at, where its cursor is when it gives up), because the
// message and the position depend on both.
//
// `__v8JsonCycleMessage(value, replacer)` is the message of the TypeError V8's
// `JSON.stringify(value, replacer)` throws when `value` contains itself (it shows the path of the
// cycle), or null when it does not. `replacer` is a function or undefined.
//
// tests/golden/v8-json.mjs compares both with the JSON of the V8 in Node over generated inputs;
// the reference fixtures hold the same messages from workerd.
//
// Written after V8's source, whose message texts and parsing steps it follows: V8 is Copyright
// 2014, the V8 project authors, under the BSD 3-Clause license in LICENSE-v8 (see NOTICE.md).
"use strict";

const __v8JsonParseError = (function () {
  const CONTEXT = 10;
  const SPECIAL = ["[object Object]", "NaN", "Infinity", "undefined"];
  const isDigit = (c) => c >= 48 && c <= 57;
  const isSpace = (c) => c === 32 || c === 9 || c === 10 || c === 13;
  // V8's one_char_json_tokens: what a character starts.
  const tokenOf = (c) => {
    if (c === -1) return "EOS";
    if (c === 34) return "STRING";
    if (isDigit(c) || c === 45) return "NUMBER";
    if (isSpace(c)) return "WHITESPACE";
    if (c === 123) return "LBRACE";
    if (c === 125) return "RBRACE";
    if (c === 91) return "LBRACK";
    if (c === 93) return "RBRACK";
    if (c === 116) return "TRUE";
    if (c === 102) return "FALSE";
    if (c === 110) return "NULL";
    if (c === 58) return "COLON";
    if (c === 44) return "COMMA";
    return "ILLEGAL";
  };

  return function v8JsonParseError(source) {
    const text = String(source);
    const length = text.length;
    let cursor = 0;
    let failure = null;

    const at = (i) => (i < length ? text.charCodeAt(i) : -1);
    const peek = () => tokenOf(at(cursor));
    const where = () => {
      let line = 1;
      let lastBreak = 0;
      for (let i = 0; i < cursor; i++) {
        let c = text.charCodeAt(i);
        if (c === 13 && i < cursor - 1 && text.charCodeAt(i + 1) === 10) {
          i++;
          c = 10;
        }
        if (c === 13 || c === 10) {
          line++;
          lastBreak = i + 1;
        }
      }
      return ` at position ${cursor} (line ${line} column ${1 + cursor - lastBreak})`;
    };
    // ReportUnexpectedToken: a specific message, or the one V8 looks up for the token.
    const report = (token, specific) => {
      if (failure !== null) return;
      if (specific) {
        failure = specific + where();
        return;
      }
      if (token === "EOS") failure = "Unexpected end of JSON input";
      else if (token === "NUMBER") failure = "Unexpected number in JSON" + where();
      else if (token === "STRING") failure = "Unexpected string in JSON" + where();
      else if (SPECIAL.includes(text)) failure = `"${text}" is not valid JSON`;
      else {
        const found = text[cursor];
        if (length < CONTEXT * 2 + 1) failure = `Unexpected token '${found}', "${text}" is not valid JSON`;
        else if (cursor < CONTEXT) failure = `Unexpected token '${found}', "${text.slice(0, cursor + CONTEXT)}"... is not valid JSON`;
        else if (cursor < length - CONTEXT) failure = `Unexpected token '${found}', ..."${text.slice(cursor - CONTEXT, cursor + CONTEXT)}"... is not valid JSON`;
        else failure = `Unexpected token '${found}', ..."${text.slice(cursor - CONTEXT)}" is not valid JSON`;
      }
    };
    const reportCharacter = () => report(peek());
    const skipWhitespace = () => {
      while (cursor < length && isSpace(text.charCodeAt(cursor))) cursor++;
    };
    const check = (token) => {
      skipWhitespace();
      if (peek() !== token) return false;
      cursor++;
      return true;
    };
    const expect = (token, message) => {
      if (peek() === token) cursor++;
      else report(peek(), message);
    };
    const expectNext = (token, message) => {
      skipWhitespace();
      expect(token, message);
    };

    // The cursor is after the opening quote.
    const scanString = () => {
      for (;;) {
        while (cursor < length) {
          const c = text.charCodeAt(cursor);
          if (c === 34 || c === 92 || c < 32) break;
          cursor++;
        }
        if (cursor >= length) return report("ILLEGAL", "Unterminated string in JSON");
        const c = text.charCodeAt(cursor);
        if (c === 34) {
          cursor++;
          return;
        }
        if (c === 92) {
          cursor++;
          const e = at(cursor);
          if (e < 0 || e > 255) return reportCharacter();
          if (e === 117) {
            for (let i = 0; i < 4; i++) {
              cursor++;
              const h = at(cursor);
              const hex = (h >= 48 && h <= 57) || (h >= 65 && h <= 70) || (h >= 97 && h <= 102);
              if (!hex) return report("ILLEGAL", "Bad Unicode escape in JSON");
            }
          } else if (!(e === 34 || e === 92 || e === 47 || e === 98 || e === 102 || e === 110 || e === 114 || e === 116)) {
            return report("ILLEGAL", "Bad escaped character in JSON");
          }
          cursor++;
          continue;
        }
        return report("ILLEGAL", "Bad control character in string literal in JSON");
      }
    };

    const scanLiteral = (word) => {
      if (text.startsWith(word, cursor)) {
        cursor += word.length;
        return;
      }
      const remaining = length - cursor;
      cursor++;
      for (let i = 0; i < Math.min(word.length - 1, remaining - 1); i++) {
        if (word.charCodeAt(1 + i) !== text.charCodeAt(cursor)) return reportCharacter();
        cursor++;
      }
      report("EOS");
    };

    const scanNumber = () => {
      let negative = false;
      if (at(cursor) === 45) {
        cursor++;
        negative = true;
      }
      if (at(cursor) === 48) {
        cursor++;
        const c = at(cursor);
        if (isDigit(c)) return report("NUMBER");
        if (!(c === 46 || c === 101 || c === 69) && !negative) return;
      } else {
        const start = cursor;
        while (isDigit(at(cursor))) cursor++;
        if (cursor === start) return report("ILLEGAL", "No number after minus sign in JSON");
      }
      if (at(cursor) === 46) {
        cursor++;
        if (!isDigit(at(cursor))) return report("ILLEGAL", "Unterminated fractional number in JSON");
        while (isDigit(at(cursor))) cursor++;
      }
      const c = at(cursor);
      if (c === 101 || c === 69) {
        cursor++;
        if (at(cursor) === 45 || at(cursor) === 43) cursor++;
        if (!isDigit(at(cursor))) return report("ILLEGAL", "Exponent part is missing a number in JSON");
        while (isDigit(at(cursor))) cursor++;
      }
    };

    // ParseJsonValue: `stack` holds the open containers ("object" or "array").
    const stack = [];
    parsing: for (;;) {
      // Produce a value.
      for (;;) {
        skipWhitespace();
        const token = peek();
        if (token === "STRING") {
          cursor++;
          scanString();
        } else if (token === "NUMBER") {
          scanNumber();
        } else if (token === "LBRACE") {
          cursor++;
          if (!check("RBRACE")) {
            stack.push("object");
            expectNext("STRING", "Expected property name or '}' in JSON");
            if (failure === null) scanString();
            if (failure === null) expectNext("COLON", "Expected ':' after property name in JSON");
            if (failure !== null) break parsing;
            continue;
          }
        } else if (token === "LBRACK") {
          cursor++;
          if (!check("RBRACK")) {
            stack.push("array");
            continue;
          }
        } else if (token === "TRUE") {
          scanLiteral("true");
        } else if (token === "FALSE") {
          scanLiteral("false");
        } else if (token === "NULL") {
          scanLiteral("null");
        } else {
          reportCharacter();
        }
        break;
      }
      if (failure !== null) break;
      // Consume the value.
      for (;;) {
        if (stack.length === 0) break parsing;
        const top = stack[stack.length - 1];
        if (top === "object") {
          if (check("COMMA")) {
            expectNext("STRING", "Expected double-quoted property name in JSON");
            if (failure === null) scanString();
            if (failure === null) expectNext("COLON", "Expected ':' after property name in JSON");
            if (failure !== null) break parsing;
            break;
          }
          expect("RBRACE", "Expected ',' or '}' after property value in JSON");
        } else {
          if (check("COMMA")) break;
          expect("RBRACK", "Expected ',' or ']' after array element in JSON");
        }
        if (failure !== null) break parsing;
        stack.pop();
      }
    }
    if (failure === null && !check("EOS")) report(peek(), "Unexpected non-whitespace character after JSON");
    return failure;
  };
})();

const __v8JsonCycleMessage = (function () {
  const constructorOf = (object) => {
    try {
      const proto = Object.getPrototypeOf(object);
      const name = proto && proto.constructor && proto.constructor.name;
      return typeof name === "string" && name ? name : "Object";
    } catch (error) {
      return "Object";
    }
  };
  const keyOf = (key) => (typeof key === "number" ? `index ${key}` : key === "" ? "<anonymous>" : `property '${key}'`);
  const line = (entry) => `\n    |     ${keyOf(entry.key)} -> object with constructor '${constructorOf(entry.object)}'`;

  return function v8JsonCycleMessage(root, replacer) {
    // The objects being written, outermost first, each with the key it was reached by.
    const stack = [];
    let message = null;
    const visit = (holder, key, value) => {
      if (message !== null) return;
      if (value !== null && (typeof value === "object" || typeof value === "bigint") && typeof value.toJSON === "function") value = value.toJSON(String(key));
      if (typeof replacer === "function") value = replacer.call(holder, String(key), value);
      if (value === null || typeof value !== "object") return;
      if (value instanceof Number || value instanceof String || value instanceof Boolean) return;
      const start = stack.findIndex((entry) => entry.object === value);
      if (start >= 0) {
        // JsonStringifier::ConstructCircularStructureErrorMessage: two entries after the start,
        // an ellipsis when more than one further entry follows, and the last entry.
        message = `Converting circular structure to JSON\n    --> starting at object with constructor '${constructorOf(stack[start].object)}'`;
        const prefixEnd = Math.min(stack.length, start + 3);
        for (let i = start + 1; i < prefixEnd; i++) message += line(stack[i]);
        if (stack.length > prefixEnd + 1) message += "\n    |     ...";
        for (let i = Math.max(prefixEnd, stack.length - 1); i < stack.length; i++) message += line(stack[i]);
        message += `\n    --- ${keyOf(key)} closes the circle`;
        return;
      }
      stack.push({ key, object: value });
      if (Array.isArray(value)) {
        for (let i = 0; i < value.length && message === null; i++) visit(value, i, value[i]);
      } else {
        for (const name of Object.keys(value)) {
          if (message !== null) break;
          visit(value, name, value[name]);
        }
      }
      stack.pop();
    };
    try {
      visit({ "": root }, "", root);
    } catch (error) {
      return null;
    }
    return message;
  };
})();
