#include "MeasurementCache.h"
#include <iostream>
using namespace facebook::react;
int main() {
  MeasurementCacheKey normal{};
  normal.markdown="![synthetic](https://example.invalid/image.png)";
  normal.maxWidth=300;
  auto safe=normal;
  safe.md4cFlagsNoMedia=true;
  if (normal==safe) return 1;
  auto &cache=MeasurementCache::shared();
  cache.set(normal,{120,240});
  CachedSize out{};
  if (cache.get(safe,out)) return 2;
  cache.set(safe,{120,24});
  if (!cache.get(normal,out) || out.height!=240) return 3;
  if (!cache.get(safe,out) || out.height!=24) return 4;
  std::cout << "PASS actual measurement cache separates identical source/geometry by noMedia\n";
}
