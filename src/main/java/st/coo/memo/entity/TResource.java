package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;
import java.sql.Timestamp;


@Setter
@Getter
@Table(value = "t_resource")
public class TResource implements Serializable {

    
    @Id
    private String publicId;

    
    private Integer memoId;

    
    private Integer userId;

    
    private String fileType;

    
    private String fileName;

    
    private String fileHash;

    
    private Long size;

    
    private String internalPath;

    
    private String externalLink;

    
    private String storageType;

    
    private Timestamp created;

    
    private Timestamp updated;

    
    private String suffix;

}
