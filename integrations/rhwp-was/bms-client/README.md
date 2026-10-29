# bms-client — BMS 쪽 `RhwpConverterService`

BMS(`svnHSO_10.3_02_bms_trunk`, Java 8 · Spring 5.2)에 넣는 `DocConverter` 구현체의 **원본**이다. BMS 는 SVN 이라
여기서 관리하고, 적용할 때 아래 위치로 복사한다. BMS 쪽 커밋은 담당자가 한다.

| 이 폴더 | BMS 위치 |
| --- | --- |
| `src/main/java/com/hs/gwweb/converter/RhwpConverterService.java` | `src/main/java/com/hs/gwweb/converter/RhwpConverterService.java` |

BMS 에서 함께 고치는 것(2026-10-29 trunk 작업본에 적용됨, 미커밋):

1. `DocConvererServiceFactory.java` — 주입과 분기 추가
   ```java
   @Autowired
   @Qualifier("RhwpConverterService")
   private DocConverter rhwpDocConverter;
   …
   if(docconverterClassName.indexOf("Rhwp") > -1) {
       logger.debug("Using rhwpDocConverter");
       return rhwpDocConverter;
   } else if(docconverterClassName.indexOf("HsCellDocConverter") > -1) {
   ```
2. `bmsWeb/WEB-INF/config/spring/globals_tpl.properties` — `docconverter.rhwp.*` 키(상위 README 의 "BMS 연동" 절).

컴파일 확인: `mvn -o dependency:build-classpath` 로 얻은 클래스패스 + `target/classes` 로 `javac -source 8` 통과(2026-10-29).
동작 확인(BMS 기동, 결재 한 바퀴, 대체 경로·브레이커)은 아직이다.
