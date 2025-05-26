
```json5
{
	"name": 2
	
}




```




We want to record `/api/instance/update`  , but it gets trapped in a snowballing loop.

It happens when we record the database queries from `/api/instance/update` and export them in a ExecutionRecording.

Then those land in the JSON request of to the same endpoint. We need to detect the "depth".

