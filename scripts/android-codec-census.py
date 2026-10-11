#!/usr/bin/env python3
"""Inspect runtime AVC codecs and Media3 choices without modifying app or device settings."""
from pathlib import Path
import argparse
import datetime, hashlib, json, os, re, subprocess

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("label", help="Unique output label; existing evidence is preserved")
parser.add_argument("serial", help="Explicit ADB device serial")
parser.add_argument("--sdk", type=Path, default=os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT"))
parser.add_argument("--java-home", type=Path, default=os.environ.get("JAVA_HOME"))
arguments = parser.parse_args()
label, serial = arguments.label, arguments.serial
if arguments.sdk is None or arguments.java_home is None:
    parser.error("Set ANDROID_HOME and JAVA_HOME, or pass --sdk and --java-home")
sdk, java = arguments.sdk.resolve(), arguments.java_home.resolve()
assert (sdk / "platforms/android-36/android.jar").is_file(), "Install platforms;android-36"
assert (sdk / "build-tools/36.0.0/d8").is_file(), "Install build-tools;36.0.0"
assert (java / "bin/javac").is_file(), "A JDK is required"
(root / "build").mkdir(exist_ok=True)
assert re.fullmatch(r'[a-z0-9-]+', label) and re.fullmatch(r'[A-Za-z0-9_.:-]+', serial)
prefix = root / 'build' / ('android-experience-' + label + '-codec-census')
assert not prefix.with_suffix('.json').exists()
work = Path(str(prefix) + '-tools');work.mkdir()
source = work / 'CodexCodecCensus.java'
source.write_text('''import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import org.json.JSONObject;
import org.json.JSONArray;
public class CodexCodecCensus {
  public static void main(String[] args) throws Exception {
    JSONObject result = new JSONObject();
    result.put("sdk", android.os.Build.VERSION.SDK_INT);
    JSONArray codecs = new JSONArray();
    for (MediaCodecInfo info : new MediaCodecList(MediaCodecList.ALL_CODECS).getCodecInfos()) {
      for (String type : info.getSupportedTypes()) {
        if (!type.equalsIgnoreCase("video/avc")) continue;
        JSONObject row = new JSONObject();
        row.put("name", info.getName()); row.put("encoder", info.isEncoder());
        MediaCodecInfo.CodecCapabilities caps = info.getCapabilitiesForType(type);
        JSONArray colors = new JSONArray();
        for (int color : caps.colorFormats) colors.put(color);
        row.put("colorFormats", colors);
        row.put("supports160x120", caps.getVideoCapabilities().isSizeSupported(160, 120));
        codecs.put(row);
      }
    }
    result.put("avcCodecs", codecs);
    try {
      Object all = Class.forName("androidx.media3.transformer.EncoderUtil").getMethod("getSupportedEncoders", String.class).invoke(null, "video/avc");
      JSONArray names = new JSONArray();
      for (Object info : (Iterable<?>) all) names.put(((MediaCodecInfo) info).getName());
      result.put("media3SupportedAvcEncoders", names);
      Class<?> selector = Class.forName("androidx.media3.transformer.EncoderSelector");
      Object preferred = selector.getMethod("selectEncoderInfos", String.class).invoke(selector.getField("DEFAULT").get(null), "video/avc");
      names = new JSONArray();
      for (Object info : (Iterable<?>) preferred) names.put(((MediaCodecInfo) info).getName());
      result.put("media3DefaultAvcEncoders", names);
    } catch (Throwable error) { result.put("media3ReflectionError", error.toString()); }
    System.out.println(result.toString(2));
  }
}
''')
env = os.environ.copy();env['JAVA_HOME'] = str(java)
with Path(str(prefix) + '-compile.log').open('w') as log:
    subprocess.run([str(java/'bin/javac'), '-source', '8', '-target', '8', '-classpath', str(sdk/'platforms/android-36/android.jar'), '-d', str(work), str(source)], env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    jar = work / 'census.jar'
    subprocess.run([str(sdk/'build-tools/36.0.0/d8'), '--lib', str(sdk/'platforms/android-36/android.jar'), '--output', str(jar), str(work/'CodexCodecCensus.class')], env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
adb = [str(sdk / 'platform-tools/adb'), '-s', serial]
apk = subprocess.check_output(adb + ['shell', 'pm', 'path', 'xyz.tironi.zoen'], text=True).strip().removeprefix('package:')
assert apk.startswith('/data/app/') and '\n' not in apk
remote = '/data/local/tmp/codex-' + label + '-codec-census.jar'
subprocess.run(adb + ['push', str(jar), remote], stdout=subprocess.DEVNULL, check=True)
try:
    result = subprocess.run(adb + ['shell', 'CLASSPATH=' + remote + ':' + apk, 'app_process', '/system/bin', 'CodexCodecCensus'], capture_output=True, text=True, timeout=30, check=True)
    Path(str(prefix) + '-device.log').write_text(result.stdout + result.stderr)
    record = json.loads(result.stdout[result.stdout.index('{'):])
    record.update(at=datetime.datetime.now(datetime.timezone.utc).isoformat(), serial=serial, source_sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(), working_diff_sha256=hashlib.sha256(subprocess.check_output(['git','diff','HEAD'],cwd=root)).hexdigest(), premise='The failed selectors assumed another usable AVC encoder was advertised by MediaCodecList; XML entries alone do not establish runtime availability.')
    record['stagefright_ccodec'] = subprocess.check_output(adb + ['shell', 'getprop', 'debug.stagefright.ccodec'], text=True).strip()
    prefix.with_suffix('.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps(record, indent=2))
finally:
    subprocess.run(adb + ['shell', 'rm', remote], check=True)
