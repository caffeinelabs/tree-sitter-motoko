/**
 * @file Motoko grammar for tree-sitter
 * @author Christoph Hegemann <christoph.hegemann@caffeine.ai>
 * @license Apache-2.0
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

function comma_sep(rule) {
  return seq(repeat(seq(rule, ",")), optional(rule));
}

// The contents of `<...>` in type parameters and instantiations: a
// plain list, or `system` followed by `, item` for each item.
function system_or_comma_sep(rule) {
  return choice(
    comma_sep(rule),
    seq("system", repeat(seq(",", rule))),
  );
}

function semi_sep(rule) {
  return seq(repeat(seq(rule, ";")), optional(rule));
}

function semi_sep1(rule) {
  return seq(repeat(seq(rule, ";")), rule, optional(";"));
}

// NOTE(def: object-vs-block-expression): The grammar parameterizes
// expression productions by whether or not they allow object literals
// to avoid ambiguity between blocks and objects. tree-sitter doesn't
// support this in the same way as Menhir, so we template these out by
// hand. The `_exp($, b)`, ... functions here produce $._exp_block or
// $._exp_object depending on the passed `b` parameter. We then use
// the `mk_exp`, ... functions and register both object and block
// variants on the main grammar.
//
// NOTE(def: head-mode): a third expression mode for unparenthesized control heads, where `{` always opens the body.
// The "head" mode is the condition of `if`/`while`, the scrutinee of `switch` and the collection of `for`
// when written without parentheses. It is block mode with two restrictions that make the `{` after the head
// always open the body: no record literal can appear anywhere in the head, and a call or index only continues
// the head when its `(`/`[` is glued to what precedes it (`token.immediate`). A spaced `(`/`[` starts the legacy
// bare branch instead, exactly as in the compiler's lexer. The prefix-shaped operators (`if (c) -1 else 1`,
// `if a #yes else #no`) are resolved by GLR: the head reading dies when no block follows, and where both
// readings survive `prec.dynamic` prefers the head. Head-mode nodes are aliased to the `_block` node names,
// so consumers see no new node kinds.
function _exp($, b) {
  return $[`_exp_${b}`]
}

// Head-mode variants of the postfix/binary productions are registered
// as `<name>_head` and aliased to `<name>_block` wherever referenced.
function mode_rule($, name, b) {
  if (b == "head") {
    return alias($[`${name}_head`], $[`${name}_block`]);
  }
  return $[`${name}_${b}`];
}

// Head-mode variants of the unparameterized prefix forms are aliased
// back to the plain node name.
function head_alias($, name, b) {
  if (b == "head") {
    return alias($[`${name}_head`], $[name]);
  }
  return $[name];
}

function _exp_non_dec($, b) {
  return $[`_exp_non_dec_${b}`]
}

function _exp_bin($, b) {
  return $[`_exp_bin_${b}`]
}

function _exp_nullary($, b) {
  return $[`_exp_nullary_${b}`]
}

function _exp_unary($, b) {
  return $[`_exp_unary_${b}`]
}

function _exp_post($, b) {
  return $[`_exp_post_${b}`]
}

function assign_exp($, b) {
  return $[`assign_exp_${b}`]
}

function array_idx_exp($, b) {
  return mode_rule($, "array_idx_exp", b)
}

function proj_exp($, b) {
  return mode_rule($, "proj_exp", b)
}

function dot_exp($, b) {
  return mode_rule($, "dot_exp", b)
}

function call_exp($, b) {
  return mode_rule($, "call_exp", b)
}

function bang_exp($, b) {
  return mode_rule($, "bang_exp", b)
}

function system_exp($, b) {
  return mode_rule($, "system_exp", b)
}

function bin_exp($, b) {
  return mode_rule($, "bin_exp", b)
}

function annot_exp($, b) {
  return mode_rule($, "annot_exp", b)
}

function binassign_exp($, b) {
  return $[`binassign_exp_${b}`]
}

function coalesce_exp($, b) {
  return mode_rule($, "coalesce_exp", b)
}

// NOTE(id: object-vs-block-expression): Conditionally allows parsing
// object literals
function mk_exp_nullary($, b) {
  if (b == "object") {
    return choice(
      $._exp_plain,
      $.var_exp,
      $.prim_exp,
      $.wild_exp,
      $.object_exp,
    );
  } else {
    return choice(
      $._exp_plain,
      $.var_exp,
      $.prim_exp,
      $.wild_exp,
    );
  }
}

// Inline me when we kill "all declarations are expressions"
function mk_exp_non_dec($, b) {
  return choice(
    _exp_bin($, b),
    assign_exp($, b),
    binassign_exp($, b),
    coalesce_exp($, b),
    $.return_exp,
    $.async_exp,
    $.asyncstar_exp,
    $.await_exp,
    $.awaitstar_exp,
    $.awaitquest_exp,
    $.assert_exp,
    $.label_exp,
    $.break_exp,
    $.continue_exp,
    $.debug_exp,
    $.if_exp,
    $.throw_exp,
    $.try_exp,
    $.switch_exp,
    $.while_exp,
    $.loop_exp,
    $.for_exp,
    $.ignore_exp,
    $.do_exp,
    $.do_quest_exp,
    $.func_exp,
  );
}

function mk_exp($, b) {
  return choice(
    $._dec_non_exp,
    _exp_non_dec($, b),
  )
}

function mk_exp_bin($, b) {
  return prec.right(choice(
    _exp_unary($, b),
    bin_exp($, b),
    annot_exp($, b),
  ))
}

// The prefix forms. In head mode their operands are head-mode too.
function mk_exp_un_ext($, b) {
  return choice(
    head_alias($, "parenthetical_exp", b),
    head_alias($, "hash_exp", b),
    head_alias($, "quest_exp", b),
    head_alias($, "unop_exp", b),
    head_alias($, "unassign_exp", b),
    $.actor_exp,
    head_alias($, "not_exp", b),
    head_alias($, "debug_show_exp", b),
    $.to_candid_exp,
    head_alias($, "from_candid_exp", b),
  );
}

function mk_exp_unary($, b) {
  return choice(
    _exp_post($, b),
    mk_exp_un_ext($, b),
  );
}

// The postfix forms that grow an expression past a single atom.
function mk_exp_post_ext($, b) {
  return choice(
    $.array_exp,
    array_idx_exp($, b),
    proj_exp($, b),
    dot_exp($, b),
    alias($._num_dot_exp, b == "object" ? $.dot_exp_object : $.dot_exp_block),
    call_exp($, b),
    bang_exp($, b),
    system_exp($, b),
  )
}

function mk_exp_post($, b) {
  return choice(
    _exp_nullary($, b),
    mk_exp_post_ext($, b),
  )
}

function mk_assign_exp($, b) {
  return seq(
    _exp_bin($, b),
    ":=",
    $._exp_object,
  )
}

function mk_array_idx_exp($, b) {
  return seq(
    _exp_post($, b),
    // NOTE(id: head-mode): only a glued `[` continues a head
    b == "head" ? token.immediate("[") : "[",
    $._exp_object,
    "]",
  )
}

function mk_proj_exp($, b) {
  return seq(
    _exp_post($, b),
    ".",
    $.proj_identifier
  )
}

function mk_dot_exp($, b) {
  return seq(
    _exp_post($, b),
    ".",
    $.identifier,
  )
}

function mk_call_exp($, b) {
  if (b == "head") {
    // NOTE(id: head-mode): in a head, an argument list continues the
    // call only when its `(` is glued (`f(x)`); `f (x)` and `f x`
    // already belong to the branch. After an explicit instantiation
    // `f<T>` the argument is any non-record atom, as in the compiler.
    return seq(
      $._exp_post_head,
      choice(
        alias($._par_exp_tight, $.par_exp),
        seq($.inst, $._exp_nullary_head),
      ),
    )
  }
  return seq(
    _exp_post($, b),
    optional($.inst),
    $._exp_nullary_object,
  )
}

function mk_bang_exp($, b) {
  return seq(
    _exp_post($, b),
    "!"
  )
}

function mk_system_exp($, b) {
  return seq(
    "(",
    "system",
    _exp_post($, b),
    ".",
    $.identifier,
    ")",
  )
}

function mk_bin_exp($, b) {
  if (b == "head") {
    // NOTE(id: head-mode): the right operand stays in head mode, and the
    // dynamic precedence makes `if a - 1 > 0 { }` read as a head rather
    // than as an atomic head `a` followed by the bare branch `-1 > 0 { }`.
    return prec.dynamic(1, prec.left(seq(
      field("left", $._exp_bin_head),
      choice($.bin_op, $.rel_op),
      field("right", $._exp_bin_head),
    )))
  }
  return prec.left(seq(
    field("left", _exp_bin($, b)),
    choice($.bin_op, $.rel_op),
    field("right", $._exp_bin_object),
  ))
}

function mk_annot_exp($, b) {
  return seq(
    _exp_bin($, b),
    ":",
    $._typ_no_bin,
  )
}

function mk_binassign_exp($, b) {
  return seq(
    _exp_bin($, b),
    $.binassign_op,
    $._exp_object,
  )
}

// The right-hand side of `??` is expression position:
// `opt ?? { x = 0 }` is a record and a block is spelled `opt ?? do { }`.
function mk_coalesce_exp($, b) {
  return prec.right(seq(
    field("value", _exp_bin($, b)),
    $._coalesce_op,
    field("default", b == "head" ? $._head : $._exp_object),
  ))
}

export default grammar({
  name: "motoko",
  extras: $ => [/\s+/, $.doc_comment, $.line_comment, $.block_comment],
  word: $ => $.identifier,
  conflicts: $ => [
    [$.var_exp, $.var_pat],
    // NOTE(id: head-mode): after an atomic head, a prefix-shaped `#`, `-`,
    // `+` or `^` may continue the head as a binary operator or start the
    // legacy bare branch; GLR keeps both until the block (or its absence)
    // decides, and `prec.dynamic` on the head reading breaks ties.
    [$._exp_post_head, $.if_exp],
    [$._exp_post_head, $.while_exp],
  ],

  rules: {
    source_file: $ => seq(
      semi_sep($.import),
      semi_sep($._dec),
    ),

    // Comments
    doc_comment: $ => token(seq("///", /[^\n]*/)),
    line_comment: $ => token(seq("//", /[^\n]*/)),
    block_comment: $ => seq("/*", optional($.comment_text), "*/"),
    comment_text: $ => repeat1(/.|\n|\r/),

    // Identifiers
    identifier: $ => /[a-zA-Z_][a-zA-Z_0-9]*/,
    _type_identifier: $ => alias($.identifier, $.type_identifier),
    tag_identifier: $ => seq("#", $.identifier),
    privileged_identifier: $ => seq("@", $.identifier),
    proj_identifier: $ => token.immediate(/[0-9]+/),

    // Literals
    text_literal: $ => /"(?:\\[unrt\\"'0-9a-fA-F]|[^\\"])*"/,
    char_literal: $ => /'\\''|'[^']*'/,
    // NOTE(def: unsigned-literals): number literals carry no sign. As in
    // the compiler's lexer, `-1` is the unary operator applied to `1`
    // (`unop_exp` / `unop_pat`); otherwise `n -1` would lex as `n` applied
    // to `-1` and `if k-1 > 0 { }` could not read `k-1` as a subtraction.
    // NOTE(def: num-dot): as in the compiler's lexer, a decimal number
    // glued to `.` and an identifier is a field access on the literal
    // (`5.toText()`, even `5.e3`), not a float. Lacking lookahead, the
    // bare-dot float `5.` is its own token, `_num_dot`, which
    // `_num_dot_exp` can continue. The other forms need a digit or a
    // signed exponent after the `.`, and all a leading digit, so `_.x`
    // stays a field of `_`.
    float_literal: $ => choice(
      token(choice(
        /[0-9][0-9_]*\.[0-9][0-9_]*/,
        /[0-9][0-9_]*(\.[0-9][0-9_]*)?[eE][+-]?[0-9_]+/,
        /[0-9][0-9_]*\.[eE][+-][0-9_]+/,
        /0x[0-9a-fA-F_]+\.[0-9a-fA-F_]*/,
        /0x[0-9a-fA-F_]+(\.[0-9a-fA-F_]*)?[pP][+-]?[0-9_]+/,
      )),
      $._num_dot,
    ),
    _num_dot: $ => /[0-9][0-9_]*\./,
    int_literal: $ => /[0-9_]+/,
    hex_literal: $ => /0x[0-9a-fA-F_]+/,
    bool_literal: $ => choice("true", "false"),
    null_literal: $ => "null",
    _literal: $ => choice(
      $.text_literal,
      $.char_literal,
      $.float_literal,
      $.int_literal,
      $.hex_literal,
      $.bool_literal,
      $.null_literal,
    ),

    // Misc
    typ_annot: $ => prec.right(seq(":", $._typ)),
    typ_path: $ => seq(
      repeat(seq($.identifier, ".")),
      $._type_identifier,
    ),
    path: $ => seq(
      repeat(seq($.identifier, ".")),
      $.identifier,
    ),
    _shared_pat: $ => choice(
      seq(
        "shared",
        optional("composite"),
        optional("query"),
        optional(field("shared_pat", $._pat_plain)),
      ),
      seq(
        optional("shared"),
        optional("composite"),
        "query",
        optional(field("shared_pat", $._pat_plain)),
      )
    ),

    unop: $ => choice(
      "+",
      "-",
      "^",
    ),
    unassign_op: $ => choice(
      "+=",
      "-=",
      "^=",
    ),
    binassign_op: $ => choice(
      "+=",
      "-=",
      "*=",
      "/=",
      "%=",
      "#=",
      "**=",
      "+%=",
      "-%=",
      "*%=",
      "**%=",
      "&=",
      "|=",
      "^=",
      "<<=",
      ">>=",
      "<<>=",
      "<>>=",
      "@=",
    ),
    // TODO: Get the precedences for these right (but only if it
    // doesn't affect the state count too much)
    rel_op: $ => choice(
      "==",
      "!=",
      // NOTE(def: leading-ws-bug): The lexer in the compiler
      // distinguishes these operators from type-parameter
      // instantiations by requiring that they are surrounded with
      // whitespace. While tree-sitter let's us do that with " <" and
      // " >", it triggers the bug in
      // https://github.com/tree-sitter/tree-sitter/issues/4091. So
      // instead we mark the `<` tokens for instantiations as
      // `token.immediate`, meaning they they must not be preceeded by
      // whitespace instead.
      ">",
      "<",
      "<=",
      ">=",
      "<<",
      ">>",
      "<<>",
      "<>>",
    ),
    bin_op: $ => choice(
      "|>",
      "or",
      "and",
      "+",
      "-",
      "*",
      "/",
      "%",
      "+%",
      "-%",
      "*%",
      "|",
      "&",
      "^",
      "#",
      "**",
      "**%",
    ),

    // `??` is the coalescing operator only when whitespace follows, as in the compiler's lexer:
    // `??x` is `?(?x)`, so `if (c) ??x else y` keeps its bare branch. The whitespace is part of the token.
    _coalesce_op: $ => alias(token(/\?\?[ \t\r\n]/), "??"),

    // Imports

    import: $ => seq(
      "import",
      $._pat_nullary,
      optional("="),
      $.text_literal,
    ),

    // Declarations
    _dec: $ => choice(
      $._dec_non_exp,
      $.exp_dec,
      $.let_else_dec,
    ),
    _dec_non_exp: $ => choice(
      $.var_dec,
      $.let_dec,
      $.typ_dec,
      $.func_dec,
      $.obj_dec,
      $.class_dec,
      $.mixin_dec,
      $.include_dec,
    ),

    exp_dec: $ => $._exp_non_dec_object,

    var_dec: $ => seq(
      "var",
      $.var_pat,
      optional($.typ_annot),
      optional(seq(
        "=",
        $._exp_object,
      ))
    ),
    let_dec: $ => seq(
      "let",
      $._pat,
      optional(seq(
        "=",
        $._exp_object,
      ))
    ),
    let_else_dec: $ => seq(
      "let",
      $._pat,
      "=",
      $._exp_object,
      "else",
      $._exp_nest,
    ),
    typ_dec: $ => seq(
      "type",
      $._type_identifier,
      optional($.typ_params),
      "=",
      $._typ,
    ),
    func_dec: $ => seq(
      optional($._shared_pat),
      "func",
      field("name", $.identifier),
      field("typ_params", optional($.typ_params)),
      field("params", $._pat_plain),
      field("return_ty", optional($.typ_annot)),
      field("body", $._func_body),
    ),

    obj_dec: $ => seq(
      optional($.parenthetical),
      $._obj_sort,
      optional($.identifier),
      optional($.typ_annot),
      optional("="),
      $.obj_body,
    ),

    class_dec: $ => seq(
      optional($.parenthetical),
      optional($._shared_pat),
      optional($._obj_sort),
      "class",
      optional($._type_identifier),
      optional($.typ_params),
      $._pat_plain,
      optional($.typ_annot),
      $._class_body,
    ),
    mixin_dec: $ => seq(
      "mixin",
      optional(seq("<", "system", ">")),
      $._pat_plain,
      $.obj_body,
    ),
    include_dec: $ => seq(
      "include",
      $.identifier,
      optional(seq("<", "system", ">")),
      $._exp_object
    ),
    _obj_sort: $ => choice(
      "object",
      "module",
      seq(optional("persistent"), "actor"),
    ),

    obj_body: $ => seq(
      "{",
      semi_sep($.dec_field),
      "}",
    ),
    _class_body: $ => seq(
      optional(seq("=", optional($.identifier))),
      $.obj_body
    ),
    dec_field: $ => seq(optional($._vis), optional($._stab), $._dec),
    _vis: $ => choice("private", "public", "system"),
    _stab: $ => choice("flexible", "stable", "transient"),

    // Expressions
    _exp_block: $ => mk_exp($, "block"),
    _exp_object: $ => mk_exp($, "object"),

    _exp_non_dec_block: $ => mk_exp_non_dec($, "block"),
    _exp_non_dec_object: $ => mk_exp_non_dec($, "object"),

    _exp_post_block: $ => mk_exp_post($, "block"),
    _exp_post_object: $ => mk_exp_post($, "object"),

    _exp_nullary_block: $ => mk_exp_nullary($, "block"),
    _exp_nullary_object: $ => mk_exp_nullary($, "object"),

    _exp_unary_block: $ => mk_exp_unary($, "block"),
    _exp_unary_object: $ => mk_exp_unary($, "object"),

    _exp_bin_block: $ => mk_exp_bin($, "block"),
    _exp_bin_object: $ => mk_exp_bin($, "object"),

    // NOTE(id: head-mode)
    _exp_nullary_head: $ => mk_exp_nullary($, "head"),
    _exp_post_head: $ => mk_exp_post($, "head"),
    _exp_unary_head: $ => mk_exp_unary($, "head"),
    _exp_bin_head: $ => mk_exp_bin($, "head"),

    // An extended head: a condition or scrutinee that is more than a
    // single atom. Disjoint from `_exp_nullary_head` by construction,
    // which is what couples the head shape to braced branches in
    // `if_exp` and `while_exp` without ambiguity.
    _exp_head: $ => choice(
      mk_exp_post_ext($, "head"),
      mk_exp_un_ext($, "head"),
      bin_exp($, "head"),
      annot_exp($, "head"),
      coalesce_exp($, "head"),
      alias($.await_exp_head, $.await_exp),
      alias($.awaitstar_exp_head, $.awaitstar_exp),
      alias($.awaitquest_exp_head, $.awaitquest_exp),
      $.do_exp,
      $.do_quest_exp,
    ),

    // The one head grammar shared by `if`, `while`, `switch` and `for`:
    // an atom or an extended head.
    _head: $ => choice(
      $._exp_nullary_head,
      $._exp_head,
    ),

    _par_exp_tight: $ => seq(token.immediate("("), comma_sep($._exp_object), ")"),

    parenthetical_exp_head: $ => seq(
      $.parenthetical,
      alias($.call_exp_head, $.call_exp_block),
    ),
    hash_exp_head: $ => seq(
      $.tag_identifier,
      optional($._exp_nullary_head),
    ),
    quest_exp_head: $ => seq("?", $._exp_unary_head),
    unop_exp_head: $ => seq($.unop, $._exp_unary_head),
    unassign_exp_head: $ => seq($.unassign_op, $._exp_unary_head),
    not_exp_head: $ => seq("not", $._exp_unary_head),
    debug_show_exp_head: $ => seq("debug_show", $._exp_unary_head),
    from_candid_exp_head: $ => seq("from_candid", $._exp_unary_head),
    await_exp_head: $ => seq("await", choice($.block_exp, $._head)),
    awaitstar_exp_head: $ => seq("await*", choice($.block_exp, $._head)),
    awaitquest_exp_head: $ => seq("await?", choice($.block_exp, $._head)),

    _exp_nest: $ => choice(
      $.block_exp,
      $._exp_block,
    ),

    _exp_plain: $ => choice(
      $.lit_exp,
      $.par_exp,
    ),
    prim_exp: $ => seq("prim", $.text_literal),
    wild_exp: $ => "_",

    lit_exp: $ => $._literal,
    // NOTE(id: num-dot): the number keeps its `.`, so `5.toText` reads
    // `(dot_exp (lit_exp (int_literal "5.")) (identifier "toText"))`.
    // Unlike the compiler, `5. f` is a field access too, not the float
    // applied to `f` (always a type error): a `token.immediate`
    // identifier conflicts with every keyword and quadruples the parse
    // table.
    _num_dot_exp: $ => seq(
      alias($._num_dot_lit, $.lit_exp),
      $.identifier,
    ),
    _num_dot_lit: $ => prec(1, alias($._num_dot, $.int_literal)),
    par_exp: $ => seq("(", comma_sep($._exp_object), ")"),
    var_exp: $ => $.identifier,
    // `if` has two coupled shapes. An atomic head keeps
    // the free-form branches Motoko always had; an extended head demands
    // braced branches, with `else if` chains allowed. A record literal as
    // head needs parentheses, so the head is never in object mode.
    if_exp: $ => choice(
      prec.right(seq(
        "if",
        field("condition", $._exp_nullary_head),
        field("then", $._exp_nest),
        optional(seq(
          "else",
          field("else", $._exp_nest),
        ))
      )),
      prec.right(seq(
        "if",
        field("condition", $._exp_head),
        field("then", $.block_exp),
        optional(seq(
          "else",
          field("else", $._else_branch),
        ))
      )),
    ),
    // `else if` chains only into the braced shape, so a legacy bare-branch
    // `if` cannot ride on an extended head.
    _else_branch: $ => choice(
      $.block_exp,
      alias($._if_braced, $.if_exp),
    ),
    _if_braced: $ => prec.right(seq(
      "if",
      field("condition", $._head),
      field("then", $.block_exp),
      optional(seq(
        "else",
        field("else", $._else_branch),
      ))
    )),
    object_exp: $ => seq(
      "{",
      choice(
        semi_sep($.exp_field),
        seq(
          $._exp_post_object,
          repeat1(seq("and", $._exp_post_object))
        ),
        seq(
          $._exp_post_object,
          repeat(seq("and", $._exp_post_object)),
          "with",
          semi_sep1($.exp_field),
        ),
      ),
      "}",
    ),
    array_exp: $ => seq(
      "[",
      optional("var"),
      comma_sep($._exp_object),
      "]"
    ),
    block_exp: $ => seq(
      "{",
      semi_sep($._dec),
      "}"
    ),
    parenthetical_exp: $ => seq(
      $.parenthetical,
      $.call_exp_block,
    ),
    hash_exp: $ => seq(
      $.tag_identifier,
      optional($._exp_nullary_object),
    ),
    quest_exp: $ => seq("?", $._exp_unary_object),
    unop_exp: $ => seq(
      $.unop,
      $._exp_unary_object,
    ),
    unassign_exp: $ => seq(
      $.unassign_op,
      $._exp_unary_object,
    ),

    actor_exp: $ => seq(
      "actor",
      $._exp_plain,
    ),
    not_exp: $ => seq(
      "not",
      $._exp_unary_object,
    ),
    debug_show_exp: $ => seq(
      "debug_show",
      $._exp_unary_object,
    ),
    to_candid_exp: $ => seq(
      "to_candid",
      "(",
      comma_sep($._exp_object),
      ")",
    ),
    from_candid_exp: $ => seq(
      "from_candid",
      $._exp_unary_object,
    ),

    // TODO(def: prec.nonassoc): https://github.com/tree-sitter/tree-sitter/issues/761
    return_exp: $ => prec.right(seq(
      "return",
      optional($._exp_object),
    )),

    async_exp: $ => seq(
      optional($.parenthetical),
      "async",
      $._exp_nest,
    ),

    asyncstar_exp: $ => seq(
      "async*",
      $._exp_nest,
    ),

    await_exp: $ => seq(
      "await",
      $._exp_nest,
    ),

    awaitstar_exp: $ => seq(
      "await*",
      $._exp_nest,
    ),

    awaitquest_exp: $ => seq(
      "await?",
      $._exp_nest,
    ),

    assert_exp: $ => seq(
      "assert",
      $._exp_nest,
    ),

    label_exp: $ => seq(
      "label",
      field("label", $.identifier),
      optional($.typ_annot),
      $._exp_nest,
    ),

    // `break l e` takes any expression, like `return e`
    // TODO(id: prec.nonassoc)
    break_exp: $ => prec.right(seq(
      "break",
      optional(seq(
        field("label", $.identifier),
        optional($._exp_object),
      )),
    )),

    continue_exp: $ => seq("continue", optional(field("label", $.identifier))),
    debug_exp: $ => seq("debug", $._exp_nest),
    throw_exp: $ => seq("throw", $._exp_nest),
    // TODO(id: prec.nonassoc)
    try_exp: $ => prec.right(seq(
      "try",
      $._exp_nest,
      optional($.catch),
      optional($.finally),
    )),
    ignore_exp: $ => seq("ignore", $._exp_nest),

    // TODO(id: prec.nonassoc)
    loop_exp: $ => prec.right(seq(
      "loop",
      field("body", $._exp_nest),
      optional(seq(
        "while",
        field("condition", $._exp_nest),
      ))
    )),
    while_exp: $ => choice(
      seq(
        "while",
        field("condition", $._exp_nullary_head),
        field("body", $._exp_nest),
      ),
      seq(
        "while",
        field("condition", $._exp_head),
        field("body", $.block_exp),
      ),
    ),
    // `for (p in e) body` is the legacy form; `for p in e { }` delimits
    // the pattern by `in` and the head by `{`.
    for_exp: $ => choice(
      seq(
        "for",
        "(",
        field("pattern", $._pat),
        "in",
        field("iterator", $._exp_object),
        ")",
        field("body", $._exp_nest),
      ),
      seq(
        "for",
        field("pattern", $._pat),
        "in",
        field("iterator", $._head),
        field("body", $.block_exp),
      ),
    ),
    do_exp: $ => seq("do", $.block_exp),
    do_quest_exp: $ => seq("do", "?", $.block_exp),

    // The `;` between cases is optional: every case starts
    // with the `case` keyword, so the separator disambiguates nothing.
    switch_exp: $ => seq(
      "switch",
      field("scrutinee", $._head),
      "{",
      repeat(seq($.case, optional(";"))),
      "}",
    ),

    func_exp: $ => seq(
      optional($._shared_pat),
      "func",
      field("typ_params", optional($.typ_params)),
      field("params", $._pat_plain),
      field("return_ty", optional($.typ_annot)),
      field("body", $._func_body),
    ),

    _func_body: $ => choice(
      seq("=", $._exp_object),
      $.block_exp,
    ),

    assign_exp_block: $ => mk_assign_exp($, "block"),
    assign_exp_object: $ => mk_assign_exp($, "object"),

    array_idx_exp_block: $ => mk_array_idx_exp($, "block"),
    array_idx_exp_object: $ => mk_array_idx_exp($, "object"),

    proj_exp_block: $ => mk_proj_exp($, "block"),
    proj_exp_object: $ => mk_proj_exp($, "object"),

    dot_exp_block: $ => mk_dot_exp($, "block"),
    dot_exp_object: $ => mk_dot_exp($, "object"),

    call_exp_block: $ => mk_call_exp($, "block"),
    call_exp_object: $ => mk_call_exp($, "object"),

    bang_exp_block: $ => mk_bang_exp($, "block"),
    bang_exp_object: $ => mk_bang_exp($, "object"),

    system_exp_block: $ => mk_system_exp($, "block"),
    system_exp_object: $ => mk_system_exp($, "object"),

    bin_exp_block: $ => mk_bin_exp($, "block"),
    bin_exp_object: $ => mk_bin_exp($, "object"),

    annot_exp_block: $ => mk_annot_exp($, "block"),
    annot_exp_object: $ => mk_annot_exp($, "object"),

    binassign_exp_block: $ => mk_binassign_exp($, "block"),
    binassign_exp_object: $ => mk_binassign_exp($, "object"),

    coalesce_exp_block: $ => mk_coalesce_exp($, "block"),
    coalesce_exp_object: $ => mk_coalesce_exp($, "object"),

    // NOTE(id: head-mode)
    array_idx_exp_head: $ => mk_array_idx_exp($, "head"),
    proj_exp_head: $ => mk_proj_exp($, "head"),
    dot_exp_head: $ => mk_dot_exp($, "head"),
    call_exp_head: $ => mk_call_exp($, "head"),
    bang_exp_head: $ => mk_bang_exp($, "head"),
    system_exp_head: $ => mk_system_exp($, "head"),
    bin_exp_head: $ => mk_bin_exp($, "head"),
    annot_exp_head: $ => mk_annot_exp($, "head"),
    coalesce_exp_head: $ => mk_coalesce_exp($, "head"),

    case: $ => seq(
      "case",
      field("pattern", $._case_pat),
      field("body", $._exp_nest),
    ),

    // Case patterns that end unambiguously without parentheses:
    // `case null`, `case 0`, `case -1`, `case ?p`, `case #tag`, `case #tag(p)`.
    // A variant payload must be parenthesized so that in `case #tag { ... }` the braces are the case body.
    _case_pat: $ => choice(
      $._pat_nullary,
      alias($._case_tag_pat, $.tag_pat),
      alias($._case_quest_pat, $.quest_pat),
      $.unop_pat,
    ),
    // `case #tag (x) ...`: the parenthesized pattern is the payload, as in
    // the compiler, so a parenthesized bare body needs a payload first.
    _case_tag_pat: $ => prec.right(seq(
      $.tag_identifier,
      optional($.tup_pat),
    )),
    _case_quest_pat: $ => seq(
      "?",
      $._case_pat,
    ),
    catch: $ => seq(
      "catch",
      field("pattern", $._pat_nullary),
      field("body", $._exp_nest),
    ),

    finally: $ => seq(
      "finally",
      $._exp_nest,
    ),

    inst: $ => seq(
      // NOTE(id: leading-ws-bug)
      token.immediate("<"),
      system_or_comma_sep($._typ),
      ">",
    ),

    exp_field: $ => seq(
      optional("var"),
      $.identifier,
      optional($.typ_annot),
      optional(seq(
        "=",
        $._exp_object,
      ))
    ),

    parenthetical: $ => seq(
      "(",
      optional($._exp_post_object),
      "with",
      semi_sep($.exp_field),
      ")",
    ),

    // Types
    _typ_nullary: $ => choice(
      $.tup_typ,
      $.path_typ,
      $.array_typ,
      $.obj_typ,
      $.variant_typ,
    ),
    _typ_un: $ => choice(
      $._typ_nullary,
      $.quest_typ,
      $.weak_typ,
    ),
    _typ_pre: $ => choice(
      $._typ_un,
      $.prim_typ,
      $.async_typ,
      $._obj_sort_typ,
    ),
    _typ_no_bin: $ => choice(
      $._typ_pre,
      $.func_typ,
    ),
    _typ: $ => choice(
      $._typ_no_bin,
      $.and_typ,
      $.or_typ,
    ),
    and_typ: $ => prec.left(
      seq($._typ, "and", $._typ),
    ),
    or_typ: $ => prec.left(
      seq($._typ, "or", $._typ),
    ),
    tup_typ: $ => seq(
      "(",
      comma_sep($.typ_item),
      ")"
    ),
    path_typ: $ => seq(
      $.typ_path,
      optional(seq(
        // NOTE(id: leading-ws-bug)
        token.immediate("<"),
        comma_sep($._typ),
        ">",
      )),
    ),
    array_typ: $ => seq(
      "[",
      optional("var"),
      $._typ,
      "]",
    ),
    obj_typ: $ => seq(
      "{",
      semi_sep($._typ_field),
      "}",
    ),
    variant_typ: $ => choice(
      seq("{", "#", "}"),
      seq(
        "{",
        semi_sep1($.typ_tag),
        "}",
      ),
    ),
    quest_typ: $ => seq("?", $._typ_un),
    weak_typ: $ => seq("weak", $._typ_un),
    prim_typ: $ => seq("prim", $.text_literal),
    async_typ: $ => seq(
      choice("async", "async*"),
      $._typ_pre,
    ),
    _obj_sort_typ: $ => seq(
      choice("module", "actor", "object"),
      $.obj_typ,
    ),
    func_typ: $ => seq(
      optional("shared"),
      optional("composite"),
      optional("query"),
      optional($.typ_params),
      $._typ_un,
      "->",
      $._typ_no_bin,
    ),
    typ_item: $ => seq(
      optional(seq($.identifier, ':')),
      $._typ,
    ),
    typ_tag: $ => seq(
      $.tag_identifier,
      optional($.typ_annot),
    ),
    _typ_field: $ => choice(
      $.typ_tf,
      $.val_tf,
      $.func_tf,
    ),
    typ_tf: $ => seq(
      "type",
      $._type_identifier,
      optional($.typ_params),
      "=",
      $._typ,
    ),
    val_tf: $ => seq(
      optional("var"),
      $.identifier,
      ":",
      $._typ,
    ),
    func_tf: $ => seq(
      $.identifier,
      optional($.typ_params),
      $._typ_nullary,
      ":",
      $._typ,
    ),
    typ_params: $ => seq(
      "<",
      system_or_comma_sep($.typ_bind),
      ">",
    ),
    typ_bind: $ => seq(
      field("name", $._type_identifier),
      optional(seq(
        "<:",
        field("supertype", $._typ),
      )),
    ),

    // Patterns
    _pat: $ => $._pat_bin,

    _pat_plain: $ => choice(
      $.wild_pat,
      $.var_pat,
      $.lit_pat,
      $.tup_pat,
    ),
    _pat_nullary: $ => choice(
      $._pat_plain,
      $.obj_pat,
    ),
    _pat_un: $ => choice(
      $._pat_nullary,
      $.tag_pat,
      $.quest_pat,
      // a signed literal is a unary pattern, as in the compiler: `? -1`
      $.unop_pat,
    ),
    _pat_bin: $ => choice(
      $._pat_un,
      $.alt_pat,
      $.and_pat,
      $.annot_pat,
    ),

    wild_pat: $ => "_",
    var_pat: $ => $.identifier,
    lit_pat: $ => $._literal,
    tup_pat: $ => seq(
      "(",
      comma_sep($._pat_bin),
      ")",
    ),
    obj_pat: $ => seq(
      "{",
      semi_sep($._pat_field),
      "}",
    ),
    tag_pat: $ => seq(
      $.tag_identifier,
      optional($._pat_nullary),
    ),
    quest_pat: $ => seq(
      "?",
      $._pat_un,
    ),
    alt_pat: $ => prec.left(2, seq(
      $._pat_bin,
      "or",
      $._pat_bin,
    )),
    and_pat: $ => prec.left(3, seq(
      $._pat_bin,
      "and",
      $._pat_bin,
    )),
    annot_pat: $ => seq($._pat_bin, $.typ_annot),
    unop_pat: $ => seq($.unop, $._literal),
    _pat_field: $ => choice($.val_pat_field, $.typ_pat_field),
    val_pat_field: $ => seq(
      $.identifier,
      optional($.typ_annot),
      optional(seq("=", $._pat))
    ),
    typ_pat_field: $ => seq(
      "type",
      $._type_identifier,
    ),
  }
});
