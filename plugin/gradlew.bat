@rem Gradle wrapper for Windows
@rem Downloads Gradle if not cached.

@if "%DEBUG%"=="" @echo off

@rem Set local scope for the variables
setlocal

set DEFAULT_JVM_OPTS="-Xmx64m" "-Xms64m"

@rem Find java.exe
if defined JAVA_HOME goto findJavaFromJavaHome

set JAVA_EXE=java.exe
%JAVA_EXE% -version >NUL 2>&1
if %ERRORLEVEL% equ 0 goto execute

echo ERROR: JAVA_HOME is not set and no 'java' command could be found in your PATH. >&2
goto fail

:findJavaFromJavaHome
set JAVA_HOME=%JAVA_HOME:"=%
set JAVA_EXE=%JAVA_HOME%/bin/java.exe

if exist "%JAVA_EXE%" goto execute

echo ERROR: JAVA_HOME is set to an invalid directory: %JAVA_HOME% >&2
goto fail

:execute
@rem Setup the command line

set APP_HOME=%~dp0
set WRAPPER_JAR=%APP_HOME%\gradle\wrapper\gradle-wrapper.jar

if exist "%WRAPPER_JAR%" (
    "%JAVA_EXE%" %DEFAULT_JVM_OPTS% -classpath "%WRAPPER_JAR%" org.gradle.wrapper.GradleWrapperMain %*
) else (
    echo ERROR: Gradle wrapper JAR not found. Run 'gradle wrapper' to generate it. >&2
    goto fail
)

goto end

:fail
exit /b 1

:end
endlocal
