// The globals of a Cloudflare Worker that model-written code reaches for, for the QuickJS sandbox.
// Each one here behaves as it does in workerd (checked against the reference fixtures in
// tests/golden/fixtures): same values, same error types and messages. What Workers has and this
// file does not define is absent on purpose, so code that uses it fails with a ReferenceError
// instead of running differently: Request, Response, Headers, streams, AbortController, Buffer,
// Intl, crypto.subtle, WebAssembly and the rest (docs/tools.md lists them).
//
// This file is the body of a function: `host` is Stellarator's side (see sandbox.js), and
// `__v8JsonParseError` comes from v8-json.js. Functions the model's code could replace later
// (JSON.stringify, Array.prototype.map) are captured first where this code depends on them.

const G = globalThis;
const define = (target, name, value) => Object.defineProperty(target, name, { value, writable: true, configurable: true, enumerable: false });
const defineAll = (target, members) => {
  for (const name of Object.getOwnPropertyNames(members)) define(target, name, members[name]);
};
/** WebIDL's message for a missing or mistyped argument. */
const idlError = (method, on, n, type) => new TypeError(`Failed to execute '${method}' on '${on}': parameter ${n} is not of type '${type}'.`);
const SCOPE = "ServiceWorkerGlobalScope";

// ------------------------------------------------------------------ globalThis aliases, console
define(G, "self", G);
define(G, "global", G);

// Workers' console has these methods. Cloudflare's executor replaces log, warn and error with its
// capture; the rest write to the Worker's own log, which is not part of the result.
{
  const quiet = () => {};
  const console = {};
  for (const name of [
    "debug", "error", "info", "log", "warn", "dir", "dirxml", "table", "trace", "group", "groupCollapsed", "groupEnd", "clear", "count",
    "countReset", "assert", "profile", "profileEnd", "time", "timeLog", "timeEnd", "timeStamp", "context",
  ]) {
    console[name] = quiet;
  }
  define(G, "console", console);
}

// ------------------------------------------------------------------ errors of the engine
// QuickJS and V8 word the errors the engine itself throws differently. Where V8's message is a
// fixed sentence around what QuickJS's own message already names, the message is rewritten to
// V8's when the error is made, so the code reads the same text in `e.message` and the model the
// same text in the tool's error. These are the cases (each one is in the reference corpus):
//   reading or setting a property of undefined or null; a variable used before its declaration;
//   `this` before super(); an invalid array or string length; a Symbol turned into a string or a
//   number; an object without a primitive value; toFixed, toPrecision, toExponential and
//   toString(radix) out of range; toISOString of an invalid date; reduce of an empty array; a
//   malformed URI; codemode.step called without its function.
// V8 messages that quote the source (`x.y is not a function`, `x is not iterable`, `Assignment to
// constant variable.` against a read-only property) cannot be rebuilt from QuickJS's and stay as
// QuickJS writes them.
//
// The hook is QuickJS's Error.prepareStackTrace, which it calls for every error it makes. Errors
// the code itself constructs (`new TypeError("...")`) keep their message, whatever it says: the
// constructors are wrapped to tell the two apart. The stack is written the way V8 writes one: the
// error's name and message first (read when the stack is), then `    at name (file:line:column)`.
{
  let constructing = 0;
  const wrappers = new WeakMap();
  const during = (run) => {
    constructing++;
    try {
      return run();
    } finally {
      constructing--;
    }
  };
  for (const name of ["Error", "TypeError", "RangeError", "ReferenceError", "SyntaxError", "EvalError", "URIError", "AggregateError"]) {
    const native = G[name];
    const wrapped = new Proxy(native, {
      construct: (target, args, newTarget) => during(() => Reflect.construct(target, args, newTarget)),
      apply: (target, self, args) => during(() => Reflect.apply(target, self, args)),
    });
    wrappers.set(wrapped, name);
    define(native.prototype, "constructor", wrapped);
    define(G, name, wrapped);
    // TypeError and the others inherit from the Error the code sees.
    if (name !== "Error") Object.setPrototypeOf(native, G.Error);
  }
  const nativeCapture = Error.captureStackTrace;
  define(Error, "captureStackTrace", function captureStackTrace(...args) {
    return during(() => nativeCapture.apply(this, args));
  });

  const DIGITS = {
    toFixed: "toFixed() digits argument must be between 0 and 100",
    toPrecision: "toPrecision() argument must be between 1 and 100",
    toExponential: "toExponential() argument must be between 0 and 100",
    toString: "toString() radix must be between 2 and 36",
  };
  const rewrite = (error, sites) => {
    const message = Object.getOwnPropertyDescriptor(error, "message");
    if (!message || typeof message.value !== "string") return;
    const text = message.value;
    const proto = Object.getPrototypeOf(error);
    const first = sites.length ? sites[0].getFunctionName() : null;
    let v8 = null;
    let found;
    if (proto === TypeError.prototype) {
      if ((found = /^cannot (read|set) property '([\s\S]*)' of (undefined|null)$/.exec(text)) && !found[2].startsWith("Symbol.")) {
        v8 = found[1] === "read" ? `Cannot read properties of ${found[3]} (reading '${found[2]}')` : `Cannot set properties of ${found[3]} (setting '${found[2]}')`;
      } else if (text === "cannot convert symbol to string") v8 = "Cannot convert a Symbol value to a string";
      else if (text === "cannot convert symbol to number") v8 = "Cannot convert a Symbol value to a number";
      else if (text === "toPrimitive") v8 = "Cannot convert object to primitive value";
      else if (text === "empty array" && (first === "reduce" || first === "reduceRight")) v8 = "Reduce of empty array with no initial value";
      // `await fn()` in codemode.step(name, fn), a line of Cloudflare's module: V8 names the callee.
      else if (text === "not a function" && sites.length && sites[0].getFileName() === "executor.js" && sites[0].getLineNumber() === host.config.stepLine) v8 = "fn is not a function";
    } else if (proto === ReferenceError.prototype) {
      if (text === "this is not initialized") v8 = "Must call super constructor in derived class before accessing 'this' or returning from derived constructor";
      else if ((found = /^([\s\S]+) is not initialized$/.exec(text))) v8 = `Cannot access '${found[1]}' before initialization`;
    } else if (proto === RangeError.prototype) {
      if (text === "invalid array length") v8 = "Invalid array length";
      else if (text === "invalid string length") v8 = "Invalid string length";
      else if (text === "invalid number of digits" && DIGITS[first]) v8 = DIGITS[first];
      else if (text === "radix must be between 2 and 36" && first === "toString") v8 = DIGITS.toString;
      else if (text === "Date value is NaN" && first === "toISOString") v8 = "Invalid time value";
    } else if (proto === URIError.prototype) v8 = "URI malformed";
    if (v8 !== null) define(error, "message", v8);
  };
  const frame = (site) => {
    const file = site.getFileName();
    let name = site.getFunctionName() || "";
    const user = file === "executor.js";
    if (user && name === "evaluate") name = "CodeExecutor.evaluate";
    // Everything outside the module stands where V8's own functions do.
    if (!user) return name ? `    at ${name} (<anonymous>)` : null;
    const at = `${file}:${site.getLineNumber()}:${site.getColumnNumber()}`;
    return name ? `    at ${name} (${at})` : `    at ${at}`;
  };
  // V8 writes an error's stack when it is first read, starting with the error's name and message
  // as they are then. QuickJS keeps the frames in the error and reads them through
  // Error.prototype.stack (in V8 `stack` is the error's own property).
  const stored = Object.getOwnPropertyDescriptor(Error.prototype, "stack");
  if (stored && typeof stored.get === "function") {
    Object.defineProperty(Error.prototype, "stack", {
      configurable: true,
      enumerable: false,
      get() {
        const frames = stored.get.call(this);
        if (typeof frames !== "string") return frames;
        let head;
        try {
          head = Error.prototype.toString.call(this);
        } catch (failure) {
          head = "Error";
        }
        return frames ? `${head}\n${frames}` : head;
      },
      set: stored.set,
    });
  }
  Error.prepareStackTrace = (error, sites) => {
    try {
      const made = constructing > 0;
      if (!made) rewrite(error, sites);
      let lines = sites.map(frame);
      // A constructed error starts in the code that made it, not in the wrapped constructor.
      if (made) lines = lines.slice(sites.findIndex((site) => site.getFileName() === "executor.js"));
      return lines.filter((line) => line !== null).join("\n");
    } catch (failure) {
      return "";
    }
  };
  // Set once, then out of sight: V8's Error has no such property until code assigns one.
  delete Error.prepareStackTrace;

  // Function.prototype.toString: a native function prints on one line in V8.
  const nativeToString = Function.prototype.toString;
  define(Function.prototype, "toString", function toString() {
    if (wrappers.has(this)) return `function ${wrappers.get(this)}() { [native code] }`;
    const text = nativeToString.call(this);
    const native = /^function ([^(]*)\(\) \{\n {4}\[native code\]\n\}$/.exec(text);
    return native ? `function ${native[1]}() { [native code] }` : text;
  });
}

// ------------------------------------------------------------------ DOMException
const DOM_CODES = {
  IndexSizeError: 1, HierarchyRequestError: 3, WrongDocumentError: 4, InvalidCharacterError: 5, NoModificationAllowedError: 7, NotFoundError: 8,
  NotSupportedError: 9, InUseAttributeError: 10, InvalidStateError: 11, SyntaxError: 12, InvalidModificationError: 13, NamespaceError: 14,
  InvalidAccessError: 15, TypeMismatchError: 17, SecurityError: 18, NetworkError: 19, AbortError: 20, URLMismatchError: 21, QuotaExceededError: 22,
  TimeoutError: 23, InvalidNodeTypeError: 24, DataCloneError: 25,
};
// In Workers an instance carries code, name, message and stack as its own enumerable properties.
class DOMException extends Error {
  constructor(message = "", name = "Error") {
    super();
    const stack = this.stack;
    for (const [key, value] of [["code", DOM_CODES[String(name)] ?? 0], ["name", String(name)], ["message", String(message)], ["stack", stack]]) {
      Object.defineProperty(this, key, { value, writable: true, configurable: true, enumerable: true });
    }
  }
  get [Symbol.toStringTag]() {
    return "DOMException";
  }
}
for (const [constant, code] of Object.entries({
  INDEX_SIZE_ERR: 1, DOMSTRING_SIZE_ERR: 2, HIERARCHY_REQUEST_ERR: 3, WRONG_DOCUMENT_ERR: 4, INVALID_CHARACTER_ERR: 5, NO_DATA_ALLOWED_ERR: 6,
  NO_MODIFICATION_ALLOWED_ERR: 7, NOT_FOUND_ERR: 8, NOT_SUPPORTED_ERR: 9, INUSE_ATTRIBUTE_ERR: 10, INVALID_STATE_ERR: 11, SYNTAX_ERR: 12,
  INVALID_MODIFICATION_ERR: 13, NAMESPACE_ERR: 14, INVALID_ACCESS_ERR: 15, VALIDATION_ERR: 16, TYPE_MISMATCH_ERR: 17, SECURITY_ERR: 18, NETWORK_ERR: 19,
  ABORT_ERR: 20, URL_MISMATCH_ERR: 21, QUOTA_EXCEEDED_ERR: 22, TIMEOUT_ERR: 23, INVALID_NODE_TYPE_ERR: 24, DATA_CLONE_ERR: 25,
})) {
  for (const target of [DOMException, DOMException.prototype]) Object.defineProperty(target, constant, { value: code, enumerable: true });
}
define(G, "DOMException", DOMException);

// ------------------------------------------------------------------ UTF-8
/** The bytes of `text` in UTF-8; a lone surrogate becomes U+FFFD, as TextEncoder writes it. */
function utf8Bytes(text) {
  const out = [];
  for (let i = 0; i < text.length; i++) {
    let c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
      const d = text.charCodeAt(i + 1);
      if (d >= 0xdc00 && d <= 0xdfff) {
        c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
        i++;
      }
    }
    if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
    if (c < 0x80) out.push(c);
    else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
    else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
  }
  return out;
}

/**
 * The WHATWG UTF-8 decoder: `state` carries a sequence cut by the end of a chunk. Returns the text;
 * an invalid sequence becomes U+FFFD, or throws when `fatal`.
 */
function utf8Text(bytes, state, flush, fatal) {
  let out = "";
  const chunk = [];
  const emit = (cp) => {
    if (cp >= 0x10000) {
      cp -= 0x10000;
      chunk.push(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
    } else chunk.push(cp);
    if (chunk.length >= 4096) {
      out += String.fromCharCode.apply(null, chunk);
      chunk.length = 0;
    }
  };
  const bad = () => {
    if (fatal) throw new TypeError("Failed to decode input.");
    emit(0xfffd);
  };
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (state.needed === 0) {
      if (b < 0x80) emit(b);
      else if (b >= 0xc2 && b <= 0xdf) {
        state.needed = 1;
        state.cp = b & 0x1f;
      } else if (b >= 0xe0 && b <= 0xef) {
        if (b === 0xe0) state.lower = 0xa0;
        if (b === 0xed) state.upper = 0x9f;
        state.needed = 2;
        state.cp = b & 0xf;
      } else if (b >= 0xf0 && b <= 0xf4) {
        if (b === 0xf0) state.lower = 0x90;
        if (b === 0xf4) state.upper = 0x8f;
        state.needed = 3;
        state.cp = b & 0x7;
      } else bad();
      continue;
    }
    if (b < state.lower || b > state.upper) {
      state.cp = 0;
      state.needed = 0;
      state.seen = 0;
      state.lower = 0x80;
      state.upper = 0xbf;
      bad();
      i--;
      continue;
    }
    state.lower = 0x80;
    state.upper = 0xbf;
    state.cp = (state.cp << 6) | (b & 0x3f);
    state.seen++;
    if (state.seen === state.needed) {
      emit(state.cp);
      state.cp = 0;
      state.needed = 0;
      state.seen = 0;
    }
  }
  if (flush && state.needed !== 0) {
    state.cp = 0;
    state.needed = 0;
    state.seen = 0;
    state.lower = 0x80;
    state.upper = 0xbf;
    bad();
  }
  if (chunk.length) out += String.fromCharCode.apply(null, chunk);
  return out;
}
const utf8State = () => ({ needed: 0, seen: 0, cp: 0, lower: 0x80, upper: 0xbf });

/** The bytes of a BufferSource argument, or null when it is none. */
function bytesOf(input) {
  if (input instanceof ArrayBuffer) return new Uint8Array(input);
  if (ArrayBuffer.isView(input)) return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  return null;
}

// ------------------------------------------------------------------ TextEncoder, TextDecoder
class TextEncoder {
  get encoding() {
    return "utf-8";
  }
  encode(input = "") {
    return new Uint8Array(utf8Bytes(String(input)));
  }
  encodeInto(source, destination) {
    if (!(destination instanceof Uint8Array)) throw idlError("encodeInto", "TextEncoder", 2, "Uint8Array");
    const text = String(source);
    let read = 0;
    let written = 0;
    while (read < text.length) {
      const c = text.codePointAt(read);
      const units = c >= 0x10000 ? 2 : 1;
      const bytes = utf8Bytes(text.substr(read, units));
      if (written + bytes.length > destination.length) break;
      for (const b of bytes) destination[written++] = b;
      read += units;
    }
    return { read, written };
  }
}
define(G, "TextEncoder", TextEncoder);

const WINDOWS_1252 = [
  0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d,
  0x2022, 0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
];
// The labels of the Encoding Standard, for the encodings decoded here.
const ENCODING_LABELS = {
  "utf-8": ["unicode-1-1-utf-8", "unicode11utf8", "unicode20utf8", "utf-8", "utf8", "x-unicode20utf8"],
  "utf-16le": ["csunicode", "iso-10646-ucs-2", "ucs-2", "unicode", "unicodefeff", "utf-16", "utf-16le"],
  "utf-16be": ["unicodefffe", "utf-16be"],
  "windows-1252": [
    "ansi_x3.4-1968", "ascii", "cp1252", "cp819", "csisolatin1", "ibm819", "iso-8859-1", "iso-ir-100", "iso8859-1", "iso88591", "iso_8859-1", "iso_8859-1:1987",
    "l1", "latin1", "us-ascii", "windows-1252", "x-cp1252",
  ],
};
class TextDecoder {
  #encoding;
  #fatal;
  #ignoreBOM;
  #state = utf8State();
  #pending = [];
  #started = false;
  constructor(label = "utf-8", options = {}) {
    const wanted = String(label).trim().toLowerCase();
    const found = Object.keys(ENCODING_LABELS).find((name) => ENCODING_LABELS[name].includes(wanted));
    if (!found) throw new RangeError(`"${label}" is not a valid encoding.`);
    this.#encoding = found;
    this.#fatal = Boolean(options && options.fatal);
    this.#ignoreBOM = Boolean(options && options.ignoreBOM);
  }
  get encoding() {
    return this.#encoding;
  }
  get fatal() {
    return this.#fatal;
  }
  get ignoreBOM() {
    return this.#ignoreBOM;
  }
  decode(input, options = {}) {
    let bytes = [];
    if (input !== undefined) {
      bytes = bytesOf(input);
      if (bytes === null) throw idlError("decode", "TextDecoder", 1, "(ArrayBuffer or ArrayBufferView)");
    }
    const stream = Boolean(options && options.stream);
    let text;
    if (this.#encoding === "utf-8") {
      text = utf8Text(bytes, this.#state, !stream, this.#fatal);
    } else if (this.#encoding === "windows-1252") {
      const units = [];
      for (let i = 0; i < bytes.length; i++) units.push(bytes[i] >= 0x80 && bytes[i] <= 0x9f ? WINDOWS_1252[bytes[i] - 0x80] : bytes[i]);
      text = "";
      for (let i = 0; i < units.length; i += 4096) text += String.fromCharCode.apply(null, units.slice(i, i + 4096));
    } else {
      const all = this.#pending.concat(Array.from(bytes));
      const whole = all.length - (all.length % 2);
      const units = [];
      for (let i = 0; i < whole; i += 2) units.push(this.#encoding === "utf-16le" ? all[i] | (all[i + 1] << 8) : (all[i] << 8) | all[i + 1]);
      this.#pending = stream ? all.slice(whole) : [];
      if (!stream && all.length % 2) {
        if (this.#fatal) throw new TypeError("Failed to decode input.");
        units.push(0xfffd);
      }
      text = "";
      for (let i = 0; i < units.length; i += 4096) text += String.fromCharCode.apply(null, units.slice(i, i + 4096));
    }
    if (!this.#started && text.length) {
      this.#started = true;
      if (!this.#ignoreBOM && text.charCodeAt(0) === 0xfeff) text = text.slice(1);
    }
    if (!stream) {
      this.#started = false;
      this.#state = utf8State();
    }
    return text;
  }
}
define(G, "TextDecoder", TextDecoder);

// ------------------------------------------------------------------ atob, btoa
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
function btoa(...args) {
  if (args.length === 0) throw idlError("btoa", SCOPE, 1, "String");
  const data = String(args[0]);
  let out = "";
  for (let i = 0; i < data.length; i += 3) {
    const a = data.charCodeAt(i);
    const b = i + 1 < data.length ? data.charCodeAt(i + 1) : NaN;
    const c = i + 2 < data.length ? data.charCodeAt(i + 2) : NaN;
    if (a > 255 || b > 255 || c > 255) {
      throw new DOMException("btoa() can only operate on characters in the Latin1 (ISO/IEC 8859-1) range.", "InvalidCharacterError");
    }
    out += B64[a >> 2] + B64[((a & 3) << 4) | ((b || 0) >> 4)];
    out += Number.isNaN(b) ? "=" : B64[((b & 15) << 2) | ((c || 0) >> 6)];
    out += Number.isNaN(c) ? "=" : B64[c & 63];
  }
  return out;
}
function atob(...args) {
  if (args.length === 0) throw idlError("atob", SCOPE, 1, "String");
  const invalid = () =>
    new DOMException(
      "atob() called with invalid base64-encoded data. (Only whitespace, '+', '/', alphanumeric ASCII, and up to two terminal '=' signs when the input data length is divisible by 4 are allowed.)",
      "InvalidCharacterError",
    );
  // The forgiving-base64 decode of the Infra Standard.
  let data = String(args[0]).replace(/[\t\n\f\r ]+/g, "");
  if (data.length % 4 === 0) data = data.replace(/==?$/, "");
  if (data.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(data)) throw invalid();
  let out = "";
  let buffer = 0;
  let bits = 0;
  for (let i = 0; i < data.length; i++) {
    buffer = (buffer << 6) | B64.indexOf(data[i]);
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out += String.fromCharCode((buffer >> bits) & 255);
    }
  }
  return out;
}
define(G, "btoa", btoa);
define(G, "atob", atob);

// ------------------------------------------------------------------ URL, URLSearchParams
const hexByte = (b) => "%" + (b < 16 ? "0" : "") + b.toString(16).toUpperCase();
/** application/x-www-form-urlencoded: the serializer's byte rule. */
function formEncode(text) {
  let out = "";
  for (const b of utf8Bytes(text)) {
    if (b === 0x20) out += "+";
    else if ((b >= 0x30 && b <= 0x39) || (b >= 0x41 && b <= 0x5a) || (b >= 0x61 && b <= 0x7a) || b === 0x2a || b === 0x2d || b === 0x2e || b === 0x5f) out += String.fromCharCode(b);
    else out += hexByte(b);
  }
  return out;
}
function formDecode(text) {
  const bytes = [];
  const raw = utf8Bytes(text);
  const hex = (b) => (b >= 48 && b <= 57 ? b - 48 : b >= 65 && b <= 70 ? b - 55 : b >= 97 && b <= 102 ? b - 87 : -1);
  for (let i = 0; i < raw.length; i++) {
    const b = raw[i];
    if (b === 0x2b) bytes.push(0x20);
    else if (b === 0x25 && i + 2 < raw.length && hex(raw[i + 1]) >= 0 && hex(raw[i + 2]) >= 0) {
      bytes.push(hex(raw[i + 1]) * 16 + hex(raw[i + 2]));
      i += 2;
    } else bytes.push(b);
  }
  return utf8Text(bytes, utf8State(), true, false);
}
function formParse(text) {
  const list = [];
  for (const part of text.split("&")) {
    if (part === "") continue;
    const at = part.indexOf("=");
    list.push(at < 0 ? [formDecode(part), ""] : [formDecode(part.slice(0, at)), formDecode(part.slice(at + 1))]);
  }
  return list;
}

const paramsState = new WeakMap();
class URLSearchParams {
  constructor(init = "") {
    const list = [];
    paramsState.set(this, { list, url: null });
    if (init === undefined || init === null) return;
    if (typeof init === "object" || typeof init === "function") {
      if (init instanceof URLSearchParams) {
        for (const [k, v] of paramsState.get(init).list) list.push([k, v]);
      } else if (typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const items = pair === null || pair === undefined || typeof pair[Symbol.iterator] !== "function" ? null : [...pair];
          if (items === null || items.length !== 2) throw new TypeError("Failed to construct 'URLSearchParams': Sequence initializer must only contain pair elements.");
          list.push([String(items[0]), String(items[1])]);
        }
      } else {
        for (const key of Object.keys(init)) list.push([key, String(init[key])]);
      }
      return;
    }
    const text = String(init);
    for (const pair of formParse(text.startsWith("?") ? text.slice(1) : text)) list.push(pair);
  }
  #changed() {
    const state = paramsState.get(this);
    if (state.url) state.url.__setSearchFromParams(this.toString());
  }
  get size() {
    return paramsState.get(this).list.length;
  }
  append(name, value) {
    paramsState.get(this).list.push([String(name), String(value)]);
    this.#changed();
  }
  delete(name, value) {
    const state = paramsState.get(this);
    const n = String(name);
    state.list = state.list.filter(([k, v]) => !(k === n && (value === undefined || v === String(value))));
    this.#changed();
  }
  get(name) {
    const n = String(name);
    const found = paramsState.get(this).list.find(([k]) => k === n);
    return found ? found[1] : null;
  }
  getAll(name) {
    const n = String(name);
    return paramsState.get(this).list.filter(([k]) => k === n).map(([, v]) => v);
  }
  has(name, value) {
    const n = String(name);
    return paramsState.get(this).list.some(([k, v]) => k === n && (value === undefined || v === String(value)));
  }
  set(name, value) {
    const state = paramsState.get(this);
    const n = String(name);
    const v = String(value);
    const at = state.list.findIndex(([k]) => k === n);
    if (at < 0) state.list.push([n, v]);
    else {
      state.list[at][1] = v;
      state.list = state.list.filter(([k], i) => i <= at || k !== n);
    }
    this.#changed();
  }
  sort() {
    const state = paramsState.get(this);
    // Stable, by the names' UTF-16 code units.
    state.list = state.list
      .map((pair, i) => [pair, i])
      .sort((a, b) => (a[0][0] < b[0][0] ? -1 : a[0][0] > b[0][0] ? 1 : a[1] - b[1]))
      .map(([pair]) => pair);
    this.#changed();
  }
  forEach(callback, thisArg) {
    if (typeof callback !== "function") throw idlError("forEach", "URLSearchParams", 1, "Function");
    for (const [k, v] of paramsState.get(this).list.slice()) callback.call(thisArg, v, k, this);
  }
  *entries() {
    for (const [k, v] of paramsState.get(this).list.slice()) yield [k, v];
  }
  *keys() {
    for (const [k] of paramsState.get(this).list.slice()) yield k;
  }
  *values() {
    for (const [, v] of paramsState.get(this).list.slice()) yield v;
  }
  [Symbol.iterator]() {
    return this.entries();
  }
  get [Symbol.toStringTag]() {
    return "URLSearchParams";
  }
  toString() {
    return paramsState
      .get(this)
      .list.map(([k, v]) => formEncode(k) + "=" + formEncode(v))
      .join("&");
  }
}
define(G, "URLSearchParams", URLSearchParams);

const urlState = new WeakMap();
const URL_PARTS = ["href", "origin", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];
class URL {
  constructor(...args) {
    if (args.length === 0) throw new TypeError("Failed to construct 'URL': constructor parameter 1 is not of type 'USVString'.");
    const parsed = host.url("parse", String(args[0]), args.length > 1 && args[1] !== undefined ? String(args[1]) : null, null);
    if (parsed == null) throw new TypeError("Invalid URL string.");
    urlState.set(this, { parts: JSON.parse(parsed), params: null });
  }
  static canParse(input, base) {
    return host.url("parse", String(input), base === undefined ? null : String(base), null) != null;
  }
  static parse(input, base) {
    try {
      return new URL(input, base);
    } catch (e) {
      return null;
    }
  }
  __setSearchFromParams(text) {
    const state = urlState.get(this);
    const next = host.url("set", state.parts.href, "search", text);
    if (next != null) state.parts = JSON.parse(next);
  }
  get [Symbol.toStringTag]() {
    return "URL";
  }
  get searchParams() {
    const state = urlState.get(this);
    if (state.params === null) {
      state.params = new URLSearchParams(state.parts.search);
      paramsState.get(state.params).url = this;
    }
    return state.params;
  }
  toString() {
    return urlState.get(this).parts.href;
  }
  toJSON() {
    return urlState.get(this).parts.href;
  }
}
for (const part of URL_PARTS) {
  Object.defineProperty(URL.prototype, part, {
    configurable: true,
    enumerable: true,
    get() {
      return urlState.get(this).parts[part];
    },
    set:
      part === "origin"
        ? undefined
        : function (value) {
            const state = urlState.get(this);
            const next = host.url("set", state.parts.href, part, String(value));
            if (next == null) {
              if (part === "href") throw new TypeError("Failed to set the 'href' property on 'URL': the provided value is not of type 'valid URL string'.");
              return;
            }
            state.parts = JSON.parse(next);
            if (state.params !== null && (part === "search" || part === "href")) paramsState.get(state.params).list = formParse(state.parts.search.slice(1));
          },
  });
}
Object.defineProperty(URL.prototype, "__setSearchFromParams", { enumerable: false });
define(G, "URL", URL);

// ------------------------------------------------------------------ crypto
class Crypto {
  getRandomValues(...args) {
    const view = args[0];
    if (!ArrayBuffer.isView(view)) throw idlError("getRandomValues", "Crypto", 1, "ArrayBufferView");
    const integer = [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array, BigInt64Array, BigUint64Array];
    if (!integer.some((kind) => view instanceof kind)) {
      throw new DOMException("ArrayBufferView argument to getRandomValues() must be an integer-typed view.", "TypeMismatchError");
    }
    if (view.byteLength > 65536) {
      throw new DOMException(`getRandomValues() only accepts buffers of size <= 64K but provided ${view.byteLength} bytes.`, "QuotaExceededError");
    }
    new Uint8Array(view.buffer, view.byteOffset, view.byteLength).set(host.random(view.byteLength));
    return view;
  }
  randomUUID() {
    const b = host.random(16);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let out = "";
    for (let i = 0; i < 16; i++) {
      if (i === 4 || i === 6 || i === 8 || i === 10) out += "-";
      out += (b[i] < 16 ? "0" : "") + b[i].toString(16);
    }
    return out;
  }
  get [Symbol.toStringTag]() {
    return "Crypto";
  }
}
define(G, "Crypto", Crypto);
define(G, "crypto", new Crypto());

// ------------------------------------------------------------------ structuredClone
const TYPED_ARRAYS = [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array, Float32Array, Float64Array, BigInt64Array, BigUint64Array];
if (typeof Float16Array === "function") TYPED_ARRAYS.push(Float16Array);
const ERROR_TYPES = { Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError };
const tagOf = (value) => Object.prototype.toString.call(value).slice(8, -1);
/** How V8 names a value it cannot clone: a function's source, `#<Ctor>` for an object. */
function uncloneable(value) {
  let shown;
  if (typeof value === "function" || typeof value === "symbol") shown = String(value);
  else {
    const tag = tagOf(value);
    if (tag === "Generator" || tag === "AsyncGenerator") shown = `[object ${tag}]`;
    else {
      const ctor = value && value.constructor && typeof value.constructor.name === "string" && value.constructor.name ? value.constructor.name : "Object";
      shown = `#<${tag === "Arguments" ? "Object" : ctor}>`;
    }
  }
  return new DOMException(`${shown} could not be cloned.`, "DataCloneError");
}
function cloneValue(value, seen) {
  if (value === null || (typeof value !== "object" && typeof value !== "function")) {
    if (typeof value === "symbol") throw uncloneable(value);
    return value;
  }
  if (typeof value === "function") throw uncloneable(value);
  if (seen.has(value)) return seen.get(value);
  const tag = tagOf(value);
  let copy;
  if (value instanceof Date) copy = new Date(value.getTime());
  else if (value instanceof RegExp) copy = new RegExp(value.source, value.flags);
  else if (value instanceof Boolean) copy = new Boolean(value.valueOf());
  else if (value instanceof Number) copy = new Number(value.valueOf());
  else if (value instanceof String) copy = new String(value.valueOf());
  else if (value instanceof ArrayBuffer) copy = value.slice(0);
  else if (value instanceof DataView) copy = new DataView(cloneValue(value.buffer, seen), value.byteOffset, value.byteLength);
  else if (ArrayBuffer.isView(value)) {
    const kind = TYPED_ARRAYS.find((k) => value instanceof k);
    copy = new kind(cloneValue(value.buffer, seen), value.byteOffset, value.length);
  }
  if (copy !== undefined) {
    seen.set(value, copy);
    return copy;
  }
  if (value instanceof Map) {
    copy = new Map();
    seen.set(value, copy);
    for (const [k, v] of value) copy.set(cloneValue(k, seen), cloneValue(v, seen));
    return copy;
  }
  if (value instanceof Set) {
    copy = new Set();
    seen.set(value, copy);
    for (const v of value) copy.add(cloneValue(v, seen));
    return copy;
  }
  if (value instanceof Error) {
    const kind = ERROR_TYPES[value.name] ?? Error;
    copy = new kind(String(value.message ?? ""));
    seen.set(value, copy);
    if (typeof value.stack === "string") define(copy, "stack", value.stack);
    if ("cause" in value) define(copy, "cause", cloneValue(value.cause, seen));
    return copy;
  }
  if (Array.isArray(value)) {
    copy = new Array(value.length);
    seen.set(value, copy);
    for (const key of Object.keys(value)) copy[key] = cloneValue(value[key], seen);
    return copy;
  }
  if (tag !== "Object") throw uncloneable(value);
  copy = {};
  seen.set(value, copy);
  for (const key of Object.keys(value)) copy[key] = cloneValue(value[key], seen);
  return copy;
}
function structuredClone(...args) {
  if (args.length === 0) throw idlError("structuredClone", SCOPE, 1, "Value");
  const copy = cloneValue(args[0], new Map());
  const transfer = args[1] && args[1].transfer;
  if (Array.isArray(transfer)) {
    for (const buffer of transfer) if (buffer instanceof ArrayBuffer && typeof buffer.transfer === "function") buffer.transfer();
  }
  return copy;
}
define(G, "structuredClone", structuredClone);

// ------------------------------------------------------------------ fetch
// Cloudflare's executor loads the code with `globalOutbound: null`: fetch exists and refuses.
const NO_INTERNET =
  "This worker is not permitted to access the internet via global functions like fetch(). It must use capabilities (such as bindings in 'env') to talk to the outside world.";
function fetch(...args) {
  if (args.length === 0) return Promise.reject(idlError("fetch", SCOPE, 1, "Request or string"));
  let target;
  try {
    target = args[0] instanceof URL ? args[0].href : String(args[0]);
  } catch (error) {
    return Promise.reject(error);
  }
  if (host.url("parse", target, null, null) == null) return Promise.reject(new TypeError(`Invalid URL: ${target}`));
  return Promise.reject(new Error(NO_INTERNET));
}
define(G, "fetch", fetch);

// ------------------------------------------------------------------ eval, Function
// Workers refuses to make code from strings.
{
  const refuse = () => {
    throw new EvalError("Code generation from strings disallowed for this context");
  };
  // (A strict function cannot be named `eval`, hence the computed name.)
  define(
    G,
    "eval",
    {
      ["eval"]: function () {
        refuse();
      },
    }["eval"],
  );
  const constructors = [Function, (async () => {}).constructor, function* () {}.constructor, async function* () {}.constructor];
  for (const original of constructors) {
    const name = original.name;
    const replacement = {
      [name]: function () {
        refuse();
      },
    }[name];
    replacement.prototype = original.prototype;
    Object.setPrototypeOf(replacement, Object.getPrototypeOf(original));
    Object.defineProperty(original.prototype, "constructor", { value: replacement, writable: true, configurable: true, enumerable: false });
    if (original === Function) define(G, "Function", replacement);
  }
}

// ------------------------------------------------------------------ Promise.any
// V8 gives the AggregateError of Promise.any a message; QuickJS leaves it empty.
{
  const nativeAny = Promise.any;
  define(Promise, "any", function any(iterable) {
    return nativeAny.call(this, iterable).catch((error) => {
      if (error instanceof AggregateError && error.message === "") define(error, "message", "All promises were rejected");
      throw error;
    });
  });
}

// ------------------------------------------------------------------ navigator, performance
{
  class Navigator {
    constructor() {
      this.languages = ["en"];
      this.language = "en";
      this.platform = "";
      this.hardwareConcurrency = 1;
      this.userAgent = "Cloudflare-Workers";
    }
    // With no way out of the sandbox the beacon goes nowhere; Workers still answers true.
    sendBeacon(...args) {
      if (args.length === 0 || typeof args[0] !== "string") throw idlError("sendBeacon", "Navigator", 1, "string");
      return true;
    }
    get [Symbol.toStringTag]() {
      return "Navigator";
    }
  }
  define(G, "navigator", new Navigator());

  // nodejs_compat's `process`, as far as a Worker has one: no cwd, argv or versions.
  const process = {
    nextTick(callback, ...args) {
      queueMicrotask(() => callback(...args));
    },
    // Values are stored as strings.
    env: new Proxy(
      {},
      {
        set(target, key, value) {
          target[key] = String(value);
          return true;
        },
      },
    ),
    // Ends the execution at once: nothing the code does can catch it.
    exit(code) {
      const n = Number(code);
      host.exit(Number.isFinite(n) ? Math.trunc(n) : 0);
      for (;;) {
        // Until the sandbox is stopped.
      }
    },
    getBuiltinModule() {
      return undefined;
    },
    platform: "linux",
    features: {
      cached_builtins: true, debug: false, inspector: false, ipv6: true, openssl_is_boringssl: true, require_module: false, tls: true, tls_alpn: true,
      tls_ocsp: true, tls_sni: true, typescript: false, uv: true,
    },
  };
  define(G, "process", process);

  class Scheduler {
    wait(...args) {
      if (args.length === 0) throw idlError("wait", "Scheduler", 1, "integer");
      return new Promise((resolve) => setTimeout(() => resolve(), Number(args[0])));
    }
    get [Symbol.toStringTag]() {
      return "Scheduler";
    }
  }
  define(G, "scheduler", new Scheduler());
  // A Worker's performance.now() counts from timeOrigin, which is 0: it reads as the clock does.
  const clock = G.Date.now;
  class Performance {
    now() {
      return clock();
    }
    get timeOrigin() {
      return 0;
    }
    get [Symbol.toStringTag]() {
      return "Performance";
    }
  }
  define(G, "performance", new Performance());
}

// ------------------------------------------------------------------ JSON.parse, JSON.stringify
// V8's error messages where they are a closed set: every JSON.parse failure, and JSON.stringify
// meeting a BigInt or a value that contains itself (v8-json.js). The parsing and writing stay
// QuickJS's.
{
  const nativeStringify = JSON.stringify;
  const reachesBigInt = (value, seen) => {
    if (typeof value === "bigint") return typeof BigInt.prototype.toJSON !== "function";
    if (value === null || typeof value !== "object" || seen.has(value)) return false;
    seen.add(value);
    if (typeof value.toJSON === "function") return false;
    for (const key of Object.keys(value)) if (reachesBigInt(value[key], seen)) return true;
    return false;
  };
  define(JSON, "stringify", function stringify(value, replacer, space) {
    try {
      return nativeStringify(value, replacer, space);
    } catch (error) {
      if (error instanceof TypeError && typeof replacer !== "object" && reachesBigInt(value, new Set())) throw new TypeError("Do not know how to serialize a BigInt");
      if (error instanceof TypeError && error.message === "circular reference" && typeof replacer !== "object") {
        const message = __v8JsonCycleMessage(value, replacer);
        if (message !== null) throw new TypeError(message);
      }
      throw error;
    }
  });
  const nativeParse = JSON.parse;
  define(JSON, "parse", function parse(text, reviver) {
    const source = String(text);
    try {
      return nativeParse(source, reviver);
    } catch (error) {
      if (error instanceof SyntaxError) {
        const message = __v8JsonParseError(source);
        if (message !== null) throw new SyntaxError(message);
      }
      throw error;
    }
  });
}

// ------------------------------------------------------------------ Date in UTC, en-US formats
// A Worker's clock is UTC and its default locale en-US. QuickJS takes the host's time zone, so
// the local-time methods are the UTC ones here, strings without a zone are read as UTC, and the
// string forms are V8's. Formatting with a locale or options needs Intl, which is not here.
{
  // (`Date` inside this block is the function declared below.)
  const NativeDate = G.Date;
  const proto = NativeDate.prototype;
  const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const two = (n) => (n < 10 ? "0" : "") + n;
  const year = (y) => (y < 0 ? "-" : "") + String(Math.abs(y)).padStart(4, "0");
  const get = (d, part) => proto["getUTC" + part].call(d);
  const hostOffset = proto.getTimezoneOffset;
  const valid = (d) => !Number.isNaN(proto.getTime.call(d));
  const datePart = (d) => `${DAYS[get(d, "Day")]} ${MONTHS[get(d, "Month")]} ${two(get(d, "Date"))} ${year(get(d, "FullYear"))}`;
  const timePart = (d) => `${two(get(d, "Hours"))}:${two(get(d, "Minutes"))}:${two(get(d, "Seconds"))} GMT+0000 (Coordinated Universal Time)`;
  const localeDate = (d) => {
    const y = get(d, "FullYear");
    return `${get(d, "Month") + 1}/${get(d, "Date")}/${y > 0 ? y : 1 - y}`;
  };
  const localeTime = (d) => {
    const h = get(d, "Hours");
    return `${h % 12 === 0 ? 12 : h % 12}:${two(get(d, "Minutes"))}:${two(get(d, "Seconds"))} ${h < 12 ? "AM" : "PM"}`;
  };
  /** toLocaleString and friends take the default locale only: [], undefined, "en-US" or "en", and no options. */
  const defaultLocale = (method, locales, options) => {
    const plain = locales === undefined || locales === "en-US" || locales === "en" || (Array.isArray(locales) && locales.length === 0);
    if (!plain || options !== undefined) {
      throw new TypeError(`${method}() with a locale or options needs Intl, which this sandbox does not have. Without arguments it formats as en-US.`);
    }
  };

  // A string without a zone is local time, which is UTC here: QuickJS read it in the host's zone.
  const hasZone = (text) =>
    /(?:Z|GMT|UTC|UT|[ECMP][SD]T)\s*(?:[+-]\d{1,2}(?::?\d{2})?)?\s*(?:\([^)]*\))?\s*$/i.test(text) ||
    /\d{1,2}:\d{2}(?::\d{2}(?:\.\d+)?)?\s*[+-]\d{2}(?::?\d{2})?\s*(?:\([^)]*\))?\s*$/.test(text);
  const dateOnly = (text) => /^\s*[+-]?\d{4,6}(?:-\d{2}(?:-\d{2})?)?\s*$/.test(text);
  const parse = (text) => {
    const source = String(text);
    const t = NativeDate.parse(source);
    if (Number.isNaN(t) || hasZone(source) || dateOnly(source)) return t;
    return t - hostOffset.call(new NativeDate(t)) * 60000;
  };

  function Date(...args) {
    if (!new.target) return toString.call(new NativeDate(NativeDate.now()));
    let made;
    if (args.length === 0) made = new NativeDate(NativeDate.now());
    else if (args.length === 1) {
      let value = args[0];
      if (value instanceof NativeDate) made = new NativeDate(proto.getTime.call(value));
      else {
        if (value !== null && (typeof value === "object" || typeof value === "function")) {
          // ToPrimitive with the default hint.
          const prim = value[Symbol.toPrimitive];
          if (typeof prim === "function") value = prim.call(value, "default");
          else {
            const own = value.valueOf();
            value = own === null || (typeof own !== "object" && typeof own !== "function") ? own : String(value);
          }
        }
        made = new NativeDate(typeof value === "string" ? parse(value) : value);
      }
    } else made = new NativeDate(NativeDate.UTC(...args));
    Object.setPrototypeOf(made, new.target.prototype);
    return made;
  }
  function toString() {
    return valid(this) ? `${datePart(this)} ${timePart(this)}` : "Invalid Date";
  }
  Date.prototype = proto;
  define(proto, "constructor", Date);
  defineAll(Date, { now: NativeDate.now, UTC: NativeDate.UTC, parse });
  Object.defineProperty(Date, "length", { value: 7, configurable: true });
  const methods = {
    toString,
    toDateString() {
      return valid(this) ? datePart(this) : "Invalid Date";
    },
    toTimeString() {
      return valid(this) ? timePart(this) : "Invalid Date";
    },
    toLocaleString(locales, options) {
      defaultLocale("toLocaleString", locales, options);
      return valid(this) ? `${localeDate(this)}, ${localeTime(this)}` : "Invalid Date";
    },
    toLocaleDateString(locales, options) {
      defaultLocale("toLocaleDateString", locales, options);
      return valid(this) ? localeDate(this) : "Invalid Date";
    },
    toLocaleTimeString(locales, options) {
      defaultLocale("toLocaleTimeString", locales, options);
      return valid(this) ? localeTime(this) : "Invalid Date";
    },
    getTimezoneOffset() {
      return valid(this) ? 0 : NaN;
    },
    getYear() {
      return get(this, "FullYear") - 1900;
    },
    setYear(y) {
      const n = Number(y);
      return proto.setUTCFullYear.call(this, Number.isInteger(n) && n >= 0 && n <= 99 ? 1900 + n : n);
    },
  };
  for (const part of ["FullYear", "Month", "Date", "Day", "Hours", "Minutes", "Seconds", "Milliseconds"]) {
    methods["get" + part] = proto["getUTC" + part];
    if (part !== "Day") methods["set" + part] = proto["setUTC" + part];
  }
  defineAll(proto, methods);
  define(G, "Date", Date);

  // Numbers in en-US: digits in groups of three, at most three decimals, rounded half away from zero.
  const group = (digits) => {
    let out = "";
    for (let i = 0; i < digits.length; i++) {
      if (i > 0 && (digits.length - i) % 3 === 0) out += ",";
      out += digits[i];
    }
    return out;
  };
  /** The decimal digits of a finite number, without exponent: [integer part, fraction part]. */
  const plainDigits = (n) => {
    let [mantissa, exponent] = Math.abs(n).toExponential().split("e");
    let digits = mantissa.replace(".", "");
    const point = 1 + Number(exponent);
    if (point <= 0) return ["0", "0".repeat(-point) + digits];
    if (point >= digits.length) return [digits + "0".repeat(point - digits.length), ""];
    return [digits.slice(0, point), digits.slice(point)];
  };
  define(Number.prototype, "toLocaleString", function toLocaleString(locales, options) {
    defaultLocale("toLocaleString", locales, options);
    const n = Number(this);
    if (Number.isNaN(n)) return "NaN";
    if (!Number.isFinite(n)) return n < 0 ? "-∞" : "∞";
    let [whole, fraction] = plainDigits(n);
    if (fraction.length > 3) {
      const up = fraction.charCodeAt(3) >= 53;
      fraction = fraction.slice(0, 3);
      if (up) {
        const bumped = String(BigInt(whole + fraction) + 1n).padStart(whole.length + 3, "0");
        whole = bumped.slice(0, bumped.length - 3);
        fraction = bumped.slice(-3);
      }
    }
    fraction = fraction.replace(/0+$/, "");
    const negative = n < 0 || Object.is(n, -0);
    return (negative ? "-" : "") + group(whole) + (fraction ? "." + fraction : "");
  });
  define(BigInt.prototype, "toLocaleString", function toLocaleString(locales, options) {
    defaultLocale("toLocaleString", locales, options);
    const text = BigInt(this).toString();
    return text.startsWith("-") ? "-" + group(text.slice(1)) : group(text);
  });
}

// ------------------------------------------------------------------ String.prototype.localeCompare
// V8 compares with ICU's root collation: punctuation, digits, then letters, case last. Done here
// for ASCII and for Latin letters with marks (compared by their base letter first), with the
// `sensitivity` and `numeric` options; the locale is not looked at (the root order is the order
// of most locales for this text). Other text falls back to QuickJS's own comparison.
{
  const ORDER = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789";
  const native = String.prototype.localeCompare;
  const keyOf = (text) => {
    const base = [];
    const marks = [];
    const cases = [];
    for (const ch of text.normalize("NFD")) {
      const code = ch.codePointAt(0);
      if (code >= 0x300 && code <= 0x36f) {
        if (marks.length) marks[marks.length - 1] += ch;
        continue;
      }
      const at = ORDER.indexOf(ch);
      const lower = ch.toLowerCase();
      if (at >= 0) base.push(at);
      else if (lower >= "a" && lower <= "z" && lower.length === 1) base.push(ORDER.length + lower.charCodeAt(0) - 97);
      else return null;
      marks.push("");
      cases.push(ch === lower ? 0 : 1);
    }
    return { base, marks, cases };
  };
  const byLevel = (x, y) => {
    const n = Math.min(x.length, y.length);
    for (let i = 0; i < n; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
    return x.length === y.length ? 0 : x.length < y.length ? -1 : 1;
  };
  const compare = (a, b, sensitivity) => {
    const ka = keyOf(a);
    const kb = keyOf(b);
    if (ka === null || kb === null) return native.call(a, b);
    return (
      byLevel(ka.base, kb.base) ||
      (sensitivity === "base" || sensitivity === "case" ? 0 : byLevel(ka.marks, kb.marks)) ||
      (sensitivity === "base" || sensitivity === "accent" ? 0 : byLevel(ka.cases, kb.cases))
    );
  };
  define(String.prototype, "localeCompare", function localeCompare(that, locales, options) {
    const a = String(this);
    const b = String(that);
    const sensitivity = options && options.sensitivity !== undefined ? String(options.sensitivity) : "variant";
    if (!(options && options.numeric)) return compare(a, b, sensitivity);
    // Runs of digits compare by their value.
    const split = (text) => text.match(/\d+|\D+/g) ?? [];
    const pa = split(a);
    const pb = split(b);
    for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
      const digits = /^\d/.test(pa[i]) && /^\d/.test(pb[i]);
      const d = digits ? (BigInt(pa[i]) < BigInt(pb[i]) ? -1 : BigInt(pa[i]) > BigInt(pb[i]) ? 1 : 0) : compare(pa[i], pb[i], sensitivity);
      if (d !== 0) return d;
    }
    return pa.length === pb.length ? 0 : pa.length < pb.length ? -1 : 1;
  });
}
