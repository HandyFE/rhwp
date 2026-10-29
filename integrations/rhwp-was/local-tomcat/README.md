# 로컬 Tomcat 인스턴스 (rhwp.war 시험용)

작업 폴더 루트의 `../../../../tomcat/` (= `C:\Handyground\Workspace\FE-rhwp\tomcat`, 저장소 밖) 를 이렇게 만들었다.

1. 기존 Tomcat 8.5.90(`C:\Handyground\hwpweb\tomcat`)의 `bin/ lib/ conf/` 만 복사, `logs/ temp/ work/ webapps/ rhwp-work/` 빈 폴더.
2. `conf/server.xml`: shutdown 8485, HTTP **8480**(AJP 8409 그대로). `conf/context.xml`: `<Manager pathname=""/>`.
3. `bin/setenv.bat` ← 이 폴더의 `setenv.bat` (rhwp.binary 는 `rhwp-FE/target/pr-review/release/rhwp.exe`, 토큰 `poc-token`).
4. `webapps/rhwp.war` ← `../target/rhwp.war`.

```bash
# 기동(콘솔 로그는 logs/console.log) / 종료 — Git Bash
cd /c/Handyground/Workspace/FE-rhwp/tomcat
export CATALINA_HOME='C:\Handyground\Workspace\FE-rhwp\tomcat' JAVA_HOME='C:\Program Files\Java\jdk-17.0.1'
cmd //c 'bin\catalina.bat run' > logs/console.log 2>&1 &
cmd //c 'bin\shutdown.bat'
# 재배포
rm -rf webapps/rhwp work/Catalina && cp ../rhwp-FE/integrations/rhwp-was/target/rhwp.war webapps/
```

주의: Git Bash 의 `curl -F 'fields=["서명.2"]'` 처럼 한글을 인자로 직접 주면 cp949 바이트가 전송되어 서버가 U+FFFD 로 읽는다.
한글이 든 `actions`/`fields` 는 UTF-8 파일로 써서 `-F 'actions=<a.json;type=application/json;charset=utf-8'` 로 보낸다.
