// BMS 가 저장한 데스크톱 한글(wordType hwp2002) 본문은 HC20 압축 + "Handysoft Approval Document File" 429바이트 헤더로
// 포장돼 있다(BMS WrtUtil.uncompressFromHC20File · ApprConstants.OLD_HEADER_LEN). 해제는 BMS 프레임워크 jar 를 그대로 쓴다.
//   javac -cp <bms>/bin/lib/hsfrmwk-3.0.jar Hc20Decode.java
//   java  -cp "<bms>/bin/lib/hsfrmwk-3.0.jar;." Hc20Decode <경로 목록 파일>
// 경로 목록 파일은 "입력 경로\n출력 경로\n" 쌍의 반복(UTF-8). 출력은 헤더까지 벗긴 HWP/HWPX 바이트이고
// 확장자는 내용(CFB → .hwp, PK → .hwpx)에 맞춰 바꿔 쓴다.
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public class Hc20Decode {
    static final byte[] OLD_HEADER = "Handysoft Approval Document File".getBytes(StandardCharsets.US_ASCII);
    static final int OLD_HEADER_LEN = 429;

    public static void main(String[] a) throws Exception {
        List<String> lines = Files.readAllLines(Paths.get(a[0]), StandardCharsets.UTF_8);
        for (int i = 0; i + 1 < lines.size(); i += 2) {
            File in = new File(lines.get(i).trim());
            String outPath = lines.get(i + 1).trim();
            try {
                File tmp = File.createTempFile("hc20", ".bin");
                com.hs.frmwk.hnet.util.FileUtils.uncompressFromHC20File(in, tmp);
                byte[] b = Files.readAllBytes(tmp.toPath());
                tmp.delete();
                int off = startsWith(b, OLD_HEADER) ? OLD_HEADER_LEN : 0;
                String ext = (b.length > off + 1 && b[off] == 'P' && b[off + 1] == 'K') ? ".hwpx" : ".hwp";
                outPath = outPath.replaceAll("\.(hwpx?|bin)$", "") + ext;
                Files.write(Paths.get(outPath), Arrays.copyOfRange(b, off, b.length));
                System.out.println("OK " + in.getName() + " -> " + outPath + " (" + (b.length - off) + "B)");
            } catch (Throwable t) {
                System.out.println("FAIL " + in.getName() + " " + t);
            }
        }
    }

    static boolean startsWith(byte[] b, byte[] p) {
        if (b.length < p.length) return false;
        for (int i = 0; i < p.length; i++) if (b[i] != p[i]) return false;
        return true;
    }
}
