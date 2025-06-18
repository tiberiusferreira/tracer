
```json5
{
	"name": 2
	
}




```




We want to record `/api/instance/update`  , but it gets trapped in a snowballing loop.

It happens when we record the database queries from `/api/instance/update` and export them in a ExecutionRecording.

Then those land in the JSON request of to the same endpoint. We need to detect the "depth".

http://127.0.0.1:4200/execution-details/37735537-17e5-4977-9cd8-6b7e83a4adf5


