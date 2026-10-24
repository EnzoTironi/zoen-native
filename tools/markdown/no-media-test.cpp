#include "MD4CParser.hpp"
#include <algorithm>
#include <cstdlib>
#include <functional>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <vector>
using namespace Markdown;
using Node = std::shared_ptr<MarkdownASTNode>;
static void check(bool value, const std::string &message) { if (!value) throw std::runtime_error(message); }
static size_t count(const Node &node, NodeType type) {
  if (!node) return 0;
  size_t total = node->type == type;
  for (auto &child : node->children) total += count(child, type);
  return total;
}
static std::string text(const Node &node) {
  if (!node) return {};
  std::string result = node->content;
  for (auto &child : node->children) result += text(child);
  return result;
}
static std::string canonical(const Node &node) {
  std::ostringstream out; out << int(node->type) << ':' << node->content.size() << ':' << node->content;
  std::vector<std::pair<std::string,std::string>> attrs(node->attributes.begin(), node->attributes.end());
  std::sort(attrs.begin(),attrs.end());
  for (auto &p : attrs) out << ':' << p.first.size() << ':' << p.first << ':' << p.second.size() << ':' << p.second;
  out << '['; for (auto &child : node->children) out << canonical(child); out << ']'; return out.str();
}
static Node parse(const std::string &source, bool noMedia) { Md4cFlags flags; flags.noMedia = noMedia; MD4CParser parser; return parser.parse(source,flags,true); }
static void noMedia(const std::string &source) {
  const auto ast = parse(source,true);
  check(count(ast,NodeType::Image) == 0,"Image node survived noMedia");
  check(count(ast,NodeType::Video) == 0,"Video node survived noMedia");
}
static int run(int argc, char **argv) {
  const bool defaultsOnly = argc == 2 && std::string(argv[1]) == "--defaults-only";
  const std::vector<std::pair<std::string,std::string>> media = {
    {"remote image","![remote](https://example.invalid/image.png)"},
    {"local image","![local](file:///private/synthetic-file.png)"},
    {"data image","![data](data:image/png;base64,AA)"},
    {"reference image","![reference][asset]\n\n[asset]: https://example.invalid/image.png"},
    {"collapsed reference","![asset][]\n\n[asset]: https://example.invalid/image.png"},
    {"shortcut reference","![asset]\n\n[asset]: https://example.invalid/image.png"},
    {"nested link","[![alt](https://example.invalid/image.png)](https://example.invalid/page)"},
    {"nested image alt","![outer ![inner](data:image/png;base64,AA)](https://example.invalid/outer.png)"},
    {"table image","| Cover |\n| --- |\n| ![table](https://example.invalid/table.png) |"},
    {"list image","- ![list](https://example.invalid/list.png)\n- ordinary"},
    {"quoted image","> ![quote](https://example.invalid/quote.png)"},
    {"HTML video","<video src=\"https://example.invalid/video.mp4\"></video>"},
    {"HTML image","<img src=\"https://example.invalid/image.png\">"},
    {"HTML case","<VIDEO src=\"file:///private/synthetic-video.mp4\"></VIDEO>"},
  };
  for (auto &test : media) {
    const auto plain = parse(test.second,false);
    if (defaultsOnly) { std::cout << test.first << ' ' << canonical(plain) << '\n'; continue; }
    noMedia(test.second);
    if (test.first.find("HTML") == std::string::npos) check(count(plain,NodeType::Image)>0,"Default image behavior changed");
    std::cout << "PASS " << test.first << '\n';
  }
  if (defaultsOnly) return 0;
  check(!Md4cFlags{}.noMedia,"noMedia must default false");
  Md4cFlags commonFlags; commonFlags.noMedia=true; MD4CParser commonParser;
  const auto common=commonParser.parse("![CommonMark](https://example.invalid/common.png)",commonFlags,false);
  check(count(common,NodeType::Image)==0 && count(common,NodeType::Video)==0,"CommonMark bypassed noMedia");
  const auto link = parse("![explicit alt](https://example.invalid/image.png)",true);
  check(count(link,NodeType::Link)>0 && text(link).find("explicit alt")!=std::string::npos,"Image alt must remain as explicit navigation text");
  const std::string code="if (x < y) {\n  const char *source = \"![literal](file:///not-read)\";\n}\n";
  const auto fenced=parse("# Heading\n\n```cpp\n"+code+"```\n\n**Strong** and *emphasis*\n\n- one\n- two\n\n| A | B |\n| --- | --- |\n| value | exact |",true);
  check(count(fenced,NodeType::CodeBlock)==1,"Code block lost");
  check(text(fenced).find(code)!=std::string::npos,"Literal code fence bytes changed");
  check(count(fenced,NodeType::Heading)==1 && count(fenced,NodeType::Strong)==1 && count(fenced,NodeType::Emphasis)==1,"Formatting lost");
  check(count(fenced,NodeType::UnorderedList)==1 && count(fenced,NodeType::Table)==1,"List/table lost");
  check(count(fenced,NodeType::Image)==0 && count(fenced,NodeType::Video)==0,"Literal code became media");
  const auto inlineCode=parse("`x < y && ![literal](file:///not-read)`",true);
  check(count(inlineCode,NodeType::Code)==1 && text(inlineCode)=="x < y && ![literal](file:///not-read)","Inline code changed");
  const std::string autolinks="See https://example.invalid/help, www.example.invalid and person@example.invalid.";
  const auto linked=parse(autolinks,true);
  check(Md4cFlags{}.permissiveAutolinks,"Pinned parser must enable its existing autolinks by default");
  check(count(linked,NodeType::Link)==3,"Bare URL, WWW or email did not become a link");
  check(text(linked)==autolinks,"Autolinking changed visible source");
  const auto codeLinks=parse("`https://example.invalid person@example.invalid`\n\n```text\nwww.example.invalid\n```",true);
  check(count(codeLinks,NodeType::Link)==0,"Literal code acquired navigation");
  noMedia(autolinks);
  std::cout << "PASS bare URL, WWW and email links with literal code preserved\n";
  MD4CParser parser; Md4cFlags defaults; const auto implicit=parser.parse("![default](https://example.invalid/default.png)");
  check(canonical(implicit)==canonical(parse("![default](https://example.invalid/default.png)",false)),"Omitted flag differs from false");
  std::cout << "PASS exact code fences, inline code, formatting, explicit links and false defaults\n";
  return 0;
}

int main(int argc, char **argv) {
  try { return run(argc,argv); }
  catch (const std::exception &error) { std::cerr << error.what() << '\n'; return 1; }
}
